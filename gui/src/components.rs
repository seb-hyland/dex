use egui::{ComboBox, Frame, Id, IntoAtoms, TextBuffer, TextEdit, text_edit::TextEditOutput};

use crate::prelude::*;

pub trait UiComponents {
    fn editable_label(&mut self, buf: &mut impl TextBuffer, id: Id) -> TextEditOutput;
    fn editable_label_with(
        &mut self,
        buf: &mut impl TextBuffer,
        id: Id,
        f: impl FnMut(TextEdit) -> TextEdit,
    ) -> TextEditOutput;

    fn time(&self) -> f64;

    fn combo_box<V: PartialEq>(
        &mut self,
        id: Id,
        current_value: &V,
        current_value_string: impl AsRef<str>,
        items: impl IntoIterator<Item = (V, impl AsRef<str>, Box<dyn FnMut(V) -> Box<dyn Action>>)>,
    ) -> Option<Box<dyn Action>>;
}

impl UiComponents for Ui {
    fn editable_label(&mut self, buf: &mut impl TextBuffer, id: Id) -> TextEditOutput {
        self.editable_label_with(buf, id, |editor| editor)
    }

    fn editable_label_with(
        &mut self,
        buf: &mut impl TextBuffer,
        id: Id,
        mut f: impl FnMut(TextEdit) -> TextEdit,
    ) -> TextEditOutput {
        let editor = TextEdit::singleline(buf)
            .background_color(Color32::TRANSPARENT)
            .id(id)
            .frame(Frame::NONE)
            .clip_text(false)
            .desired_width(0.0);
        f(editor).show(self)
    }

    fn time(&self) -> f64 {
        self.input(|i| i.time)
    }

    fn combo_box<V: PartialEq>(
        &mut self,
        id: Id,
        current_value: &V,
        current_value_string: impl AsRef<str>,
        items: impl IntoIterator<Item = (V, impl AsRef<str>, Box<dyn FnMut(V) -> Box<dyn Action>>)>,
    ) -> Option<Box<dyn Action>> {
        let mut action = None;

        ComboBox::from_id_salt(id)
            .selected_text(current_value_string.as_ref())
            .show_ui(self, |ui| {
                for (value, display_name, mut action_creator) in items {
                    if ui
                        .selectable_label(current_value == &value, display_name.as_ref())
                        .clicked()
                    {
                        action = Some(action_creator(value));
                    }
                }
            });

        action
    }
}
