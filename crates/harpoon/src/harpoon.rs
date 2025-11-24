use gpui::{
    Action, App, Context, DismissEvent, Entity, Modifiers, MouseButton, MouseUpEvent,
    ParentElement, Render, Styled, Task, WeakEntity, Window, actions, rems,
};
use picker::{Picker, PickerDelegate};
use project::Project;
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;
use ui::{DecoratedIcon, ListItem, ListItemSpacing, Tooltip, prelude::*};
use workspace::{
    Pane, Workspace,
    item::{ItemHandle, TabContentParams},
};

#[derive(Clone)]
struct TabMatch {
    pane: WeakEntity<Pane>,
    item_index: usize,
    item: Box<dyn ItemHandle>,
    detail: usize,
    preview: bool,
}

pub struct HarpoonDelegate {
    select_last: bool,
    harpoon: WeakEntity<Harpoon>,
    selected_index: usize,
    pane: WeakEntity<Pane>,
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    matches: Vec<TabMatch>,
    original_items: Vec<(Entity<Pane>, usize)>,
    is_all_panes: bool,
    restored_items: bool,
}

const PANEL_WIDTH_REMS: f32 = 28.;

/// Toggles the tab switcher interface.
#[derive(PartialEq, Clone, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = harpoon)]
#[serde(deny_unknown_fields)]
pub struct Toggle {
    #[serde(default)]
    pub select_last: bool,
}
actions!(
    tab_switcher,
    [
        /// Closes the selected item in the tab switcher.
        CloseSelectedItem,
        /// Toggles between showing all tabs or just the current pane's tabs.
        ToggleAll
    ]
);

pub struct Harpoon {
    picker: Entity<Picker<HarpoonDelegate>>,
    init_modifiers: Option<Modifiers>,
}

impl PickerDelegate for HarpoonDelegate {
    type ListItem = ListItem;

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        "Search all tabs…".into()
    }

    fn no_matches_text(&self, _window: &mut Window, _cx: &mut App) -> Option<SharedString> {
        Some("No tabs".into())
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;

        let Some(selected_match) = self.matches.get(self.selected_index()) else {
            return;
        };
        selected_match
            .pane
            .update(cx, |pane, cx| {
                if let Some(index) = pane.index_for_item(selected_match.item.as_ref()) {
                    pane.activate_item(index, false, false, window, cx);
                }
            })
            .ok();
        cx.notify();
    }

    fn separators_after_indices(&self) -> Vec<usize> {
        Vec::new()
    }

    fn update_matches(
        &mut self,
        raw_query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.update_matches(raw_query, window, cx);
        Task::ready(())
    }

    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut Context<Picker<HarpoonDelegate>>,
    ) {
        let Some(selected_match) = self.matches.get(self.selected_index()) else {
            return;
        };

        self.restored_items = true;
        for (pane, index) in self.original_items.iter() {
            pane.update(cx, |this, cx| {
                this.activate_item(*index, false, false, window, cx);
            })
        }
        selected_match
            .pane
            .update(cx, |pane, cx| {
                if let Some(index) = pane.index_for_item(selected_match.item.as_ref()) {
                    pane.activate_item(index, true, true, window, cx);
                }
            })
            .ok();
    }

    fn dismissed(&mut self, window: &mut Window, cx: &mut Context<Picker<HarpoonDelegate>>) {
        if !self.restored_items {
            for (pane, index) in self.original_items.iter() {
                pane.update(cx, |this, cx| {
                    this.activate_item(*index, false, false, window, cx);
                })
            }
        }

        self.harpoon
            .update(cx, |_, cx| cx.emit(DismissEvent))
            .log_err();
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let tab_match = self.matches.get(ix)?;

        let params = TabContentParams {
            detail: Some(tab_match.detail),
            selected: true,
            preview: tab_match.preview,
            deemphasized: false,
        };
        let label = tab_match.item.tab_content(params, window, cx);

        let icon = tab_match.icon(&self.project, selected, window, cx);

        let indicator = render_item_indicator(tab_match.item.boxed_clone(), cx);
        let indicator_color = if let Some(ref indicator) = indicator {
            indicator.color
        } else {
            Color::default()
        };
        let indicator = h_flex()
            .flex_shrink_0()
            .children(indicator)
            .child(div().w_2())
            .into_any_element();
        let close_button = div()
            .id("close-button")
            .on_mouse_up(
                // We need this on_mouse_up here because on macOS you may have ctrl held
                // down to open the menu, and a ctrl-click comes through as a right click.
                MouseButton::Right,
                cx.listener(move |picker, _: &MouseUpEvent, window, cx| {
                    cx.stop_propagation();
                    picker.delegate.close_item_at(ix, window, cx);
                }),
            )
            .child(
                IconButton::new("close_tab", IconName::Close)
                    .icon_size(IconSize::Small)
                    .icon_color(indicator_color)
                    .tooltip(Tooltip::for_action_title("Close", &CloseSelectedItem))
                    .on_click(cx.listener(move |picker, _, window, cx| {
                        cx.stop_propagation();
                        picker.delegate.close_item_at(ix, window, cx);
                    })),
            )
            .into_any_element();

        Some(
            ListItem::new(ix)
                .spacing(ListItemSpacing::Sparse)
                .inset(true)
                .toggle_state(selected)
                .child(h_flex().w_full().child(label))
                .start_slot::<DecoratedIcon>(icon)
                .map(|el| {
                    if self.selected_index == ix {
                        el.end_slot::<AnyElement>(close_button)
                    } else {
                        el.end_slot::<AnyElement>(indicator)
                            .end_hover_slot::<AnyElement>(close_button)
                    }
                }),
        )
    }
}

pub fn init(cx: &mut App) {
    cx.observe_new(Harpoon::register).detach();
}

impl Harpoon {
    fn register(
        workspace: &mut Workspace,
        _window: Option<&mut Window>,
        _: &mut Context<Workspace>,
    ) {
        workspace.register_action(|workspace, action: &Toggle, window, cx| {
            let Some(harpoon) = workspace.active_modal::<Self>(cx) else {
                Self::open(workspace, action.select_last, false, window, cx);
                return;
            };

            harpoon.update(cx, |harpoon, cx| {
                harpoon
                    .picker
                    .update(cx, |picker, cx| picker.cycle_selection(window, cx))
            });
        });
        workspace.register_action(|workspace, _action: &ToggleAll, window, cx| {
            let Some(harpoon) = workspace.active_modal::<Self>(cx) else {
                Self::open(workspace, false, true, window, cx);
                return;
            };

            harpoon.update(cx, |harpoon, cx| {
                harpoon
                    .picker
                    .update(cx, |picker, cx| picker.cycle_selection(window, cx))
            });
        });
    }
}

impl Render for Harpoon {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // v_flex()
        //     .key_context("Harpoon")
        //     .w(rems(PANEL_WIDTH_REMS))
        //     .on_modifiers_changed(cx.listener(Self::handle_modifiers_changed))
        //     .on_action(cx.listener(Self::handle_close_selected_item))
        //     .child(self.harpoon.clone())
    }
}
