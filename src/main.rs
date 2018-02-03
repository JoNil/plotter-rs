extern crate arrayvec;

extern crate glium;

#[macro_use]
extern crate imgui;
extern crate imgui_glium_renderer;

extern crate nfd;

extern crate time;

use imgui::{
    ImGui,
    ImGuiCond,
    ImGuiKey,
    Ui,
};

use imgui_glium_renderer::Renderer;

use glium::glutin::{
    Api,
    ContextBuilder,
    EventsLoop,
    GlProfile,
    GlRequest,
    WindowBuilder,
};

use glium::{
    Display,
    Surface,
};

use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

mod timer;

struct Block {
    data: arrayvec::ArrayVec<[f64; 4096]>,
}

impl Block {
    fn new() -> Block {
        Block {
            data: arrayvec::ArrayVec::new(),
        }
    }
}

struct State {

    loading: Arc<AtomicBool>,
    loading_thread: Option<thread::JoinHandle<()>>,

    blocks: Arc<Mutex<Vec<Block>>>,
}

impl State {
    fn new() -> State {
        State {
            loading: Arc::new(AtomicBool::new(false)),
            loading_thread: None,
            blocks: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

fn open_file(path: &str, state: &mut State) {

    if !state.loading.compare_and_swap(false, true, Ordering::SeqCst) {

        state.blocks.lock().unwrap().clear();

        let blocks = state.blocks.clone();
        let loading = state.loading.clone();
        let owned_path = path.to_owned();

        state.loading_thread = Some(thread::spawn(move || {

            let mut t = timer::Timer::new();

            let is_text_file = owned_path.ends_with(".txt");

            if let Ok(file) = File::open(&owned_path) {

                let reader = BufReader::new(&file);

                if is_text_file {

                    let mut block = Block::new();

                    for maybe_line in reader.lines() {

                        if let Ok(line) = maybe_line {
                            if let Ok(val) = line.parse::<f64>() {

                                if block.data.is_full() {
                                    blocks.lock().unwrap().push(block);
                                    block = Block::new();
                                }

                                block.data.push(val);
                            }
                        }
                    }

                    if block.data.len() != 0 {
                        blocks.lock().unwrap().push(block);
                    }
                } else {
                    println!("Not a text file");
                }
            }

            println!("Load time: {}", t.reset());

            loading.store(false, Ordering::SeqCst);
        }));
    }
}

fn save_file(path: &str, state: &State) {

}

fn run_ui(ui: &Ui, state: &mut State) -> bool {

    ui.window(im_str!("Main"))
        .size(ui.imgui().display_size(), ImGuiCond::Always)
        .position((0.0, 0.0), ImGuiCond::Always)
        .movable(false)
        .resizable(false)
        .title_bar(false)
        .collapsible(false)
        .menu_bar(true)
        .build(|| {

            ui.menu_bar(|| {
                ui.menu(im_str!("File")).build(|| {
                    if ui.menu_item(im_str!("Open"))
                        .enabled(!state.loading.load(Ordering::SeqCst))
                        .build() {

                        if let Ok(nfd::Response::Okay(path)) =
                            nfd::open_file_dialog(Some("txt,pf"), None) {
                            open_file(&path, state);
                        }
                    }
                    if ui.menu_item(im_str!("Save"))
                        .enabled(!state.loading.load(Ordering::SeqCst))
                        .build() {

                        if let Ok(nfd::Response::Okay(path)) =
                            nfd::open_save_dialog(Some("pf"), None) {
                            save_file(&path, state);
                        }
                    }

                    if ui.menu_item(im_str!("Close"))
                        .enabled(!state.loading.load(Ordering::SeqCst) &&
                            state.blocks.lock().unwrap().len() > 0)
                        .build() {

                        state.blocks.lock().unwrap().clear();
                    }
                });
            });

            ui.text(im_str!("Data size: {}", state.blocks.lock().unwrap().len()));

            ui.text(im_str!("Fps: {:.1} {:.2} ms", ui.framerate(), 1000.0 / ui.framerate()));
        });

    true
}

#[derive(Copy, Clone, PartialEq, Debug, Default)]
struct MouseState {
    pos: (i32, i32),
    pressed: (bool, bool, bool),
    wheel: f32,
}

fn main() {
    let mut events_loop = EventsLoop::new();

    let display = {
        let context = ContextBuilder::new()
            .with_gl_profile(GlProfile::Core)
            .with_gl(GlRequest::Specific(Api::OpenGl, (4, 3)));
        let window = WindowBuilder::new()
            .with_title("plotter-rs")
            .with_dimensions(1024, 768);
        Display::new(window, context, &events_loop).unwrap()
    };

    let mut imgui = ImGui::init();
    imgui.set_ini_filename(None);
    imgui.style_mut().window_rounding = 0.0;
    
    let mut renderer = Renderer::init(&mut imgui, &display).expect("Failed to initialize renderer");

    imgui.set_imgui_key(ImGuiKey::Tab, 0);
    imgui.set_imgui_key(ImGuiKey::LeftArrow, 1);
    imgui.set_imgui_key(ImGuiKey::RightArrow, 2);
    imgui.set_imgui_key(ImGuiKey::UpArrow, 3);
    imgui.set_imgui_key(ImGuiKey::DownArrow, 4);
    imgui.set_imgui_key(ImGuiKey::PageUp, 5);
    imgui.set_imgui_key(ImGuiKey::PageDown, 6);
    imgui.set_imgui_key(ImGuiKey::Home, 7);
    imgui.set_imgui_key(ImGuiKey::End, 8);
    imgui.set_imgui_key(ImGuiKey::Delete, 9);
    imgui.set_imgui_key(ImGuiKey::Backspace, 10);
    imgui.set_imgui_key(ImGuiKey::Enter, 11);
    imgui.set_imgui_key(ImGuiKey::Escape, 12);
    imgui.set_imgui_key(ImGuiKey::A, 13);
    imgui.set_imgui_key(ImGuiKey::C, 14);
    imgui.set_imgui_key(ImGuiKey::V, 15);
    imgui.set_imgui_key(ImGuiKey::X, 16);
    imgui.set_imgui_key(ImGuiKey::Y, 17);
    imgui.set_imgui_key(ImGuiKey::Z, 18);

    let mut frame_timer = timer::Timer::new();
    let mut mouse_state = MouseState::default();
    let mut quit = false;

    let mut state = State::new();

    loop {
        events_loop.poll_events(|event| {
            use glium::glutin::WindowEvent::*;
            use glium::glutin::ElementState::Pressed;
            use glium::glutin::{Event, MouseButton, MouseScrollDelta, TouchPhase};

            if let Event::WindowEvent { event, .. } = event {
                match event {
                    Closed => quit = true,
                    KeyboardInput { input, .. } => {
                        use glium::glutin::VirtualKeyCode as Key;

                        let pressed = input.state == Pressed;
                        match input.virtual_keycode {
                            Some(Key::Tab) => imgui.set_key(0, pressed),
                            Some(Key::Left) => imgui.set_key(1, pressed),
                            Some(Key::Right) => imgui.set_key(2, pressed),
                            Some(Key::Up) => imgui.set_key(3, pressed),
                            Some(Key::Down) => imgui.set_key(4, pressed),
                            Some(Key::PageUp) => imgui.set_key(5, pressed),
                            Some(Key::PageDown) => imgui.set_key(6, pressed),
                            Some(Key::Home) => imgui.set_key(7, pressed),
                            Some(Key::End) => imgui.set_key(8, pressed),
                            Some(Key::Delete) => imgui.set_key(9, pressed),
                            Some(Key::Back) => imgui.set_key(10, pressed),
                            Some(Key::Return) => imgui.set_key(11, pressed),
                            Some(Key::Escape) => imgui.set_key(12, pressed),
                            Some(Key::A) => imgui.set_key(13, pressed),
                            Some(Key::C) => imgui.set_key(14, pressed),
                            Some(Key::V) => imgui.set_key(15, pressed),
                            Some(Key::X) => imgui.set_key(16, pressed),
                            Some(Key::Y) => imgui.set_key(17, pressed),
                            Some(Key::Z) => imgui.set_key(18, pressed),
                            Some(Key::LControl) |
                            Some(Key::RControl) => imgui.set_key_ctrl(pressed),
                            Some(Key::LShift) |
                            Some(Key::RShift) => imgui.set_key_shift(pressed),
                            Some(Key::LAlt) | Some(Key::RAlt) => imgui.set_key_alt(pressed),
                            Some(Key::LWin) | Some(Key::RWin) => imgui.set_key_super(pressed),
                            _ => {}
                        }
                    }
                    CursorMoved { position: (x, y), .. } => mouse_state.pos = (x as i32, y as i32),
                    MouseInput { state, button, .. } => {
                        match button {
                            MouseButton::Left => mouse_state.pressed.0 = state == Pressed,
                            MouseButton::Right => mouse_state.pressed.1 = state == Pressed,
                            MouseButton::Middle => mouse_state.pressed.2 = state == Pressed,
                            _ => {}
                        }
                    }
                    MouseWheel {
                        delta: MouseScrollDelta::LineDelta(_, y),
                        phase: TouchPhase::Moved,
                        ..
                    } |
                    MouseWheel {
                        delta: MouseScrollDelta::PixelDelta(_, y),
                        phase: TouchPhase::Moved,
                        ..
                    } => mouse_state.wheel = y,
                    ReceivedCharacter(c) => imgui.add_input_character(c),
                    _ => (),
                }
            }
        });

        {
            let scale = imgui.display_framebuffer_scale();

            imgui.set_mouse_pos(
                mouse_state.pos.0 as f32 / scale.0,
                mouse_state.pos.1 as f32 / scale.1,
            );

            imgui.set_mouse_down(
                &[
                    mouse_state.pressed.0,
                    mouse_state.pressed.1,
                    mouse_state.pressed.2,
                    false,
                    false,
                ],
            );

            imgui.set_mouse_wheel(mouse_state.wheel / scale.1);
            mouse_state.wheel = 0.0;
        }

        let gl_window = display.gl_window();
        let size_pixels = gl_window.get_inner_size().unwrap();
        let size_points = {
            let hidpi = gl_window.hidpi_factor();
            ((size_pixels.0 as f32 / hidpi) as u32, (size_pixels.1 as f32 / hidpi) as u32)
        };

        let ui = imgui.frame(size_points, size_pixels, frame_timer.reset() as f32);
        if !run_ui(&ui, &mut state) {
            break;
        }

        {
            let mut target = display.draw();
            target.clear_color(0.35, 0.3, 0.3, 1.0);
            renderer.render(&mut target, ui).expect("Rendering failed");
            target.finish().unwrap();
        }

        if quit {
            break;
        }
    }    
}
