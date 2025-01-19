use gpui::{
    actions, impl_actions, rems, Action, AnyElement, AnyWindowHandle, AppContext, DismissEvent,
    EventEmitter, FocusHandle, FocusableView, Model, Modifiers, ModifiersChangedEvent, MouseButton,
    MouseUpEvent, ParentElement, Render, Styled, Task, View, ViewContext, VisualContext, WeakView,
    WindowId,
};
use picker::{Picker, PickerDelegate};
use project::Project;
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
        // let mut weak_pane = workspace.active_pane().downgrade();
        // for dock in [
        //     workspace.left_dock(),
        //     workspace.bottom_dock(),
        //     workspace.right_dock(),
        // ] {
        //     dock.update(cx, |this, cx| {
        //         let Some(panel) = this
        //             .active_panel()
        //             .filter(|panel| panel.focus_handle(cx).contains_focused(cx))
        //         else {
        //             return;
        //         };
        //         if let Some(pane) = panel.pane(cx) {
        //             weak_pane = pane.downgrade();
        //         }
        //     })
        // }

        let project = workspace.project().clone();
        workspace.toggle_modal(cx, |cx| {
            let delegate = WindowSwitcherDelegate::new(project, action, cx.view().downgrade(), cx);
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
    project: Model<Project>,
    matches: Vec<WindowMatch>,
}

impl WindowSwitcherDelegate {
    fn new(
        project: Model<Project>,
        action: &Toggle,
        tab_switcher: WeakView<WindowSwitcher>,
        cx: &mut ViewContext<WindowSwitcher>,
    ) -> Self {
        Self {
            select_last: action.select_last,
            window_switcher: tab_switcher,
            selected_index: 0,
            project,
            matches: Vec::new(),
        }
    }

    // fn subscribe_to_updates(pane: &WeakView<Pane>, cx: &mut ViewContext<WindowSwitcher>) {
    //     let Some(pane) = pane.upgrade() else {
    //         return;
    //     };
    //     cx.subscribe(&pane, |tab_switcher, _, event, cx| {
    //         match event {
    //             PaneEvent::AddItem { .. }
    //             | PaneEvent::RemovedItem { .. }
    //             | PaneEvent::Remove { .. } => tab_switcher.picker.update(cx, |picker, cx| {
    //                 let selected_item_id = picker.delegate.selected_item_id();
    //                 picker.delegate.update_matches(cx);
    //                 if let Some(item_id) = selected_item_id {
    //                     picker.delegate.select_item(item_id, cx);
    //                 }
    //                 cx.notify();
    //             }),
    //             _ => {}
    //         };
    //     })
    //     .detach();
    // }

    fn update_matches(&mut self, cx: &mut WindowContext) {
        self.matches.clear();

        let windows = cx.windows();

        // let pane = pane.read(cx);
        // let mut history_indices = HashMap::default();
        // pane.activation_history().iter().rev().enumerate().for_each(
        //     |(history_index, history_entry)| {
        //         history_indices.insert(history_entry.entity_id, history_index);
        //     },
        // );

        // let items: Vec<Box<dyn ItemHandle>> = pane.items().map(|item| item.boxed_clone()).collect();
        // items
        //     .iter()
        //     .enumerate()
        //     .zip(tab_details(&items, cx))
        //     .map(|((item_index, item), detail)| WindowMatch {
        //         item_index,
        //         item: item.boxed_clone(),
        //         detail,
        //         preview: pane.is_active_preview_item(item.item_id()),
        //     })
        //     .for_each(|tab_match| self.matches.push(tab_match));

        for window in windows {
            let title = "dummy title for now";
            let detail = format!("{:?}", window.window_id());
            let window_match = WindowMatch { window, detail };
            self.matches.push(window_match);
        }

        // let non_history_base = history_indices.len();
        // self.matches.sort_by(move |a, b| {
        //     let a_score = *history_indices
        //         .get(&a.item.item_id())
        //         .unwrap_or(&(a.item_index + non_history_base));
        //     let b_score = *history_indices
        //         .get(&b.item.item_id())
        //         .unwrap_or(&(b.item_index + non_history_base));
        //     a_score.cmp(&b_score)
        // });

        if self.matches.len() > 1 {
            if self.select_last {
                self.selected_index = self.matches.len() - 1;
            } else {
                self.selected_index = 1;
            }
        }
    }

    fn selected_item_id(&self) -> Option<WindowId> {
        self.matches
            .get(self.selected_index())
            .map(|window_match| window_match.window.window_id())
    }

    fn select_item(
        &mut self,
        item_id: WindowId,
        cx: &mut ViewContext<Picker<WindowSwitcherDelegate>>,
    ) {
        let selected_idx = self
            .matches
            .iter()
            .position(|window_match| window_match.window.window_id() == item_id)
            .unwrap_or(0);
        self.set_selected_index(selected_idx, cx);
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
        println!("focusing window: {:?}", selected_match.window.window_id());
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
        // let params = TabContentParams {
        //     detail: Some(window_match.detail),
        //     selected: true,
        //     preview: window_match.preview,
        // };
        // let label = window_match.item.tab_content(params, cx);

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
