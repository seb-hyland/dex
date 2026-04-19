use crate::prelude::*;
use crate::{actions::DoActionContext, canvas::Canvas};

use std::hash::Hash;

use egui::{
    CursorIcon, Frame, Id, TextEdit,
    text::{CCursor, CCursorRange},
};

#[derive(Clone)]
pub struct TabState {
    tabs: Option<Tabs>,
    visible: Rigid<bool>,
}

impl Default for TabState {
    fn default() -> Self {
        Self {
            tabs: None,
            visible: Rigid::from(true),
        }
    }
}

#[derive(Clone, Default)]
struct Tabs {
    tabs: Vec<Tab>,
    active: usize,
    next_index: usize,
    renaming: RenamingTab,
}

#[derive(Clone, Copy, Default)]
enum RenamingTab {
    #[default]
    None,
    Newly(usize),
    Some(usize),
}

#[derive(Clone)]
struct Tab {
    /// Display name of the tab
    name: Buffer<String>,
    canvas: Canvas,
    /// A stable index assigned at initialization for hashing purposes
    stable_index: usize,
}

pub const NEW_TAB_NAME: &str = "Unnamed desktop";

impl Hash for Tab {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.stable_index.hash(state);
    }
}

impl TabState {
    pub fn visible(&self) -> bool {
        self.visible.val()
    }

    pub fn active_canvas(&mut self) -> Option<&mut Canvas> {
        self.tabs
            .as_mut()
            .map(|tabs| &mut tabs.tabs[tabs.active].canvas)
    }
}

impl<'ctx> DoActionContext<'ctx> {
    pub fn tab_state(&mut self) -> &mut Tabs {
        self.situation.tab_state.tabs.get_or_insert_default()
    }
}

action! {
    NewTab {}
        does(ctx) {
            let tab_state = ctx.tab_state();

            let stable_index = tab_state.next_index;
            tab_state.next_index += 1;

            let new_tab = Tab {
                canvas: Canvas::default(),
                name: Buffer::new(
                    NEW_TAB_NAME.to_string(),
                    Id::new("tab_bar").with(stable_index),
                ),
                stable_index,
            };
            tab_state.tabs.push(new_tab);

            tab_state.renaming = RenamingTab::Newly(tab_state.tabs.len() - 1);
        }
}

action! {
    CloseTab { idx: usize }
        does(ctx) {
            let tab_state = ctx.tab_state();
            tab_state.tabs.remove(idx);
            if tab_state.tabs.len() == 0 {
                ctx.situation.tab_state.tabs = None;
            }
        }
}

impl TabState {
    pub fn draw_fluent(&mut self, ui: &mut Ui, actions: &mut Actions) {
        let tab_bar_button_text = if self.visible() { "⏶" } else { "⏷" };

        let tab_bar_height = if !self.visible() {
            0.0
        } else {
            egui::Panel::top("tab_bar")
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(tabs) = &mut self.tabs {
                            tabs.draw(ui, actions);
                        };

                        if ui.button("+").clicked() {
                            actions.push(NewTab {});
                        }
                    });
                })
                .response
                .rect
                .height()
        };

        egui::Area::new(Id::new("tab_bar_handle"))
            .fixed_pos(Pos2 {
                x: ui.max_rect().width() / 2.0,
                y: tab_bar_height,
            })
            .show(ui.ctx(), |ui| {
                if ui
                    .button(tab_bar_button_text)
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    self.visible.modify(|vis| *vis = !*vis);
                }
            });
    }
}

impl Tabs {
    fn draw(&mut self, ui: &mut Ui, actions: &mut Actions) {
        egui_dnd::dnd(ui, "tabs_dnd").show_vec(&mut self.tabs, |ui, tab, handle, state| {
            let idx = state.index;
            if state.dragged {
                self.active = idx;
            }
            let is_active = idx == self.active;

            handle.ui(ui, |ui| {
                // Draw with editable text
                if let RenamingTab::Newly(idx) | RenamingTab::Some(idx) = self.renaming
                    && idx == state.index
                {
                    let output = tab.name.show(|name, id| {
                        TextEdit::singleline(name)
                            .id(id)
                            .clip_text(false)
                            .desired_width(0.0)
                            .frame(Frame::NONE)
                            .show(ui)
                    });

                    action! {
                        SetTabName { idx: usize, value: String }
                            does(ctx) {
                                let tabs = ctx.situation.tab_state.tabs.as_mut().unwrap();
                                tabs.tabs.get_mut(idx).unwrap().name.set(value);
                            }
                    }
                    tab.name
                        .resolve_pending_actions(ui, actions, |s| SetTabName { idx, value: s });

                    // If we are newly renaming this tab, select the whole text region
                    if matches!(self.renaming, RenamingTab::Newly(_)) {
                        output.response.request_focus();
                        let mut state =
                            TextEdit::load_state(ui.ctx(), output.response.id).unwrap_or_default();
                        state.cursor.set_char_range(Some(CCursorRange::two(
                            CCursor::new(0),
                            CCursor::new(tab.name.len_chars()),
                        )));
                        state.store(ui.ctx(), output.response.id);

                        self.renaming = RenamingTab::Some(idx);
                    }

                    if output.response.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        self.renaming = RenamingTab::None;
                    }
                } else {
                    let label_res = ui.selectable_label(is_active, tab.name.backing_value());
                    if label_res.clicked() {
                        self.active = idx;
                    }
                    if label_res.double_clicked() {
                        self.renaming = RenamingTab::Newly(idx);
                    }
                }

                if ui.button("x").clicked() {
                    actions.push(CloseTab { idx });
                }
            });
        });
    }
}

// fn do_action(ctx: DoActionContext) {
//     let tab_state = ctx.situation.tab_state.tabs.get_or_insert_default();

//     match self {
//         TabAction::Rearrange(drag_update) => {
//             shift_vec(drag_update.from, drag_update.to, &mut tab_state.tabs);
//         }
//         TabAction::Forward => {
//             if let Some(tabs) = &mut ctx.situation.tab_state.tabs {
//                 tabs.active = (tabs.active + 1).min(tabs.tabs.len() - 1);
//             }
//         }
//         TabAction::Backward => {
//             if let Some(tabs) = &mut ctx.situation.tab_state.tabs {
//                 tabs.active = tabs.active.saturating_sub(1);
//             }
//         }
//     }
// }
