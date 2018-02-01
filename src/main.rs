extern crate glium;

#[macro_use]
extern crate imgui;
extern crate imgui_glium_renderer;

use imgui::*;

mod gui;

const CLEAR_COLOR: [f32; 4] = [0.35, 0.3, 0.3, 1.0];

fn main() {

    let mut data = ImString::with_capacity(128);

    gui::run("plotter-rs", CLEAR_COLOR, |ui: &Ui| -> bool {

        ui.window(im_str!("Main"))
            .size(ui.imgui().display_size(), ImGuiCond::Always)
            .position((0.0, 0.0), ImGuiCond::Always)
            .movable(false)
            .resizable(false)
            .title_bar(false)
            .collapsible(false)
            .build(|| {
                ui.text(im_str!("Hello world!"));
                ui.text(im_str!("This...is...imgui-rs!"));
                ui.separator();
                let mouse_pos = ui.imgui().mouse_pos();
                ui.text(im_str!(
                    "Mouse Position: ({:.1},{:.1})",
                    mouse_pos.0,
                    mouse_pos.1
                ));

                ui.input_text(im_str!("Test"), &mut data).build();
            });

        true
    });
}