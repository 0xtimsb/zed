#[cfg(test)]
mod window_switcher_tests;

use gpui::{
    actions, impl_actions, rems, Action, AnyElement, AnyWindowHandle, AppContext, DismissEvent,
    EventEmitter, FocusHandle, FocusableView, Modifiers, ModifiersChangedEvent, MouseButton,
    MouseUpEvent, ParentElement, Render, Styled, Task, View, ViewContext, VisualContext, WeakView,
};
use picker::{Picker, PickerDelegate};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;
use ui::{prelude::*, ListItem, ListItemSpacing, Tooltip};
use util::ResultExt;
use workspace::{ModalView, Workspace};

const PANEL_WIDTH_REMS: f32 = 28.;

#[derive(PartialEq, Clone, Deserialize, JsonSchema, Default)]
pub struct Toggle {
    #[serde(default)]
    pub select_last: bool,
}

impl_actions!(window_switcher, [Toggle]);
actions!(window_switcher, [CloseSelectedItem]);

pub struct WindowSwitcher {
    picker: View<Picker<WindowSwitcherDelegate>>,
    init_modifiers: Option<Modifiers>,
}

impl ModalView for WindowSwitcher {}

pub fn init(cx: &mut AppContext) {
    cx.observe_new_views(WindowSwitcher::register).detach();
}

impl WindowSwitcher {
    fn register(workspace: &mut Workspace, _: &mut ViewContext<Workspace>) {
        workspace.register_action(|workspace, action: &Toggle, cx| {
            let Some(window_switcher) = workspace.active_modal::<Self>(cx) else {
                Self::open(action, workspace, cx);
                return;
            };

            window_switcher.update(cx, |window_switcher, cx| {
                window_switcher
                    .picker
                    .update(cx, |picker, cx| picker.cycle_selection(cx))
            });
        });
    }

    fn open(action: &Toggle, workspace: &mut Workspace, cx: &mut ViewContext<Workspace>) {
        workspace.toggle_modal(cx, |cx| {
            let delegate = WindowSwitcherDelegate::new(action, cx.view().downgrade());
            WindowSwitcher::new(delegate, cx)
        });
    }

    fn new(delegate: WindowSwitcherDelegate, cx: &mut ViewContext<Self>) -> Self {
        Self {
            picker: cx.new_view(|cx| Picker::nonsearchable_uniform_list(delegate, cx)),
            init_modifiers: cx.modifiers().modified().then_some(cx.modifiers()),
        }
    }

    fn handle_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        cx: &mut ViewContext<Self>,
    ) {
        let Some(init_modifiers) = self.init_modifiers else {
            return;
        };
        if !event.modified() || !init_modifiers.is_subset_of(event) {
            self.init_modifiers = None;
            if self.picker.read(cx).delegate.matches.is_empty() {
                cx.emit(DismissEvent)
            } else {
                cx.dispatch_action(menu::Confirm.boxed_clone());
            }
        }
    }

    fn handle_close_selected_item(&mut self, _: &CloseSelectedItem, cx: &mut ViewContext<Self>) {
        self.picker.update(cx, |picker, cx| {
            picker
                .delegate
                .close_item_at(picker.delegate.selected_index(), cx)
        });
    }
}

impl EventEmitter<DismissEvent> for WindowSwitcher {}

impl FocusableView for WindowSwitcher {
    fn focus_handle(&self, cx: &AppContext) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for WindowSwitcher {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        v_flex()
            .key_context("WindowSwitcher")
            .w(rems(PANEL_WIDTH_REMS))
            .on_modifiers_changed(cx.listener(Self::handle_modifiers_changed))
            .on_action(cx.listener(Self::handle_close_selected_item))
            .child(self.picker.clone())
    }
}

struct WindowMatch {
    window: AnyWindowHandle,
    detail: String,
}

pub struct WindowSwitcherDelegate {
    select_last: bool,
    window_switcher: WeakView<WindowSwitcher>,
    selected_index: usize,
    matches: Vec<WindowMatch>,
}

impl WindowSwitcherDelegate {
    fn new(action: &Toggle, tab_switcher: WeakView<WindowSwitcher>) -> Self {
        // TODO: subscribe to window create/close events so that we can update the matches list
        Self {
            select_last: action.select_last,
            window_switcher: tab_switcher,
            selected_index: 0,
            matches: Vec::new(),
        }
    }

    fn update_matches(&mut self, cx: &mut WindowContext) {
        self.matches.clear();

        for window in cx.windows() {
            let title = cx
                .update_window(window, |_, cx| cx.title())
                .unwrap_or_else(|_| "New Window".to_string());
            let window_match = WindowMatch {
                window,
                detail: title,
            };
            self.matches.push(window_match);
        }

        if self.matches.len() > 1 {
            if self.select_last {
                self.selected_index = self.matches.len() - 1;
            } else {
                self.selected_index = 1;
            }
        }
    }

    fn close_item_at(&mut self, ix: usize, cx: &mut ViewContext<Picker<WindowSwitcherDelegate>>) {
        let Some(window_match) = self.matches.get(ix) else {
            return;
        };
        cx.update_window(window_match.window, |_, cx| {
            cx.remove_window();
        })
        .log_err();
    }
}

impl PickerDelegate for WindowSwitcherDelegate {
    type ListItem = ListItem;

    fn placeholder_text(&self, _cx: &mut WindowContext) -> Arc<str> {
        Arc::default()
    }

    fn no_matches_text(&self, _cx: &mut WindowContext) -> SharedString {
        "No windows".into()
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, ix: usize, cx: &mut ViewContext<Picker<Self>>) {
        self.selected_index = ix;
        cx.notify();
    }

    fn separators_after_indices(&self) -> Vec<usize> {
        Vec::new()
    }

    fn update_matches(
        &mut self,
        _raw_query: String,
        cx: &mut ViewContext<Picker<Self>>,
    ) -> Task<()> {
        self.update_matches(cx);
        Task::ready(())
    }

    fn confirm(&mut self, _secondary: bool, cx: &mut ViewContext<Picker<WindowSwitcherDelegate>>) {
        let Some(selected_match) = self.matches.get(self.selected_index()) else {
            return;
        };
        cx.update_window(selected_match.window, |_, cx| cx.activate_window())
            .log_err();
    }

    fn dismissed(&mut self, cx: &mut ViewContext<Picker<WindowSwitcherDelegate>>) {
        self.window_switcher
            .update(cx, |_, cx| cx.emit(DismissEvent))
            .log_err();
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        cx: &mut ViewContext<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let window_match = self
            .matches
            .get(ix)
            .expect("Invalid matches state: no element for index {ix}");

        let label = Label::new(window_match.detail.clone()).into_any_element();
        let close_button = div()
            // We need this on_mouse_up here instead of on_click on the close
            // button because Picker intercepts the same events and handles them
            // as click's on list items.
            // See the same handler in Picker for more details.
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(move |picker, _: &MouseUpEvent, cx| {
                    cx.stop_propagation();
                    picker.delegate.close_item_at(ix, cx);
                }),
            )
            .child(
                IconButton::new("close_tab", IconName::Close)
                    .icon_size(IconSize::Small)
                    .tooltip(|cx| Tooltip::text("Close", cx)),
            )
            .into_any_element();

        Some(
            ListItem::new(ix)
                .spacing(ListItemSpacing::Sparse)
                .inset(true)
                .toggle_state(selected)
                .child(h_flex().w_full().child(label))
                .map(|el| {
                    if self.selected_index == ix {
                        el.end_slot::<AnyElement>(close_button)
                    } else {
                        el.end_hover_slot::<AnyElement>(close_button)
                    }
                }),
        )
    }
}
