extern crate arrayvec;

extern crate glium;

#[macro_use]
extern crate imgui;
extern crate imgui_glium_renderer;

extern crate nfd;

extern crate time;

#[cfg(windows)]
extern crate winapi;

use imgui::{
    ImGui,
    ImGuiCond,
    ImGuiKey,
    ImVec2,
    Ui,
};

use imgui_glium_renderer::Renderer;

use glium::glutin::{
    Api,
    ContextBuilder,
    EventsLoop,
    GlContext,
    GlProfile,
    GlRequest,
    WindowBuilder,
};

use glium::{
    Display,
    Surface,
};

use std::cmp::max;
use std::fmt;
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

#[derive(Debug, Copy, Clone)]
struct MouseState {
    pos: (i32, i32),
    pressed: (bool, bool, bool),
    wheel: f32,
}

impl MouseState {
    fn new() -> MouseState {
        MouseState {
            pos: (0, 0),
            pressed: (false, false, false),
            wheel: 0.0,
        }
    }
}

struct Data {
    blocks: Arc<Mutex<Vec<Block>>>,
    points: Vec<ImVec2>,
}

impl Data {
    fn new() -> Data {
        Data {
            blocks: Arc::new(Mutex::new(Vec::new())),
            points: Vec::new(),
        }
    }
}

impl fmt::Debug for Data {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Data {{ blocks: {}: points: {} }}",
            self.blocks.lock().unwrap().len(),
            self.points.len())
    }
}

#[derive(Debug)]
struct State {

    loading: Arc<AtomicBool>,
    loading_thread: Option<thread::JoinHandle<()>>,

    data: Data,

    pan: (f32, f32),

    frame_timer: timer::Timer,

    mouse_state: MouseState,
    last_mouse_state: MouseState,

    quit: bool,
}

impl State {
    fn new() -> State {
        State {
            loading: Arc::new(AtomicBool::new(false)),
            loading_thread: None,
            data: Data::new(),
            pan: (0.0, 0.0),
            frame_timer: timer::Timer::new(),
            mouse_state: MouseState::new(),
            last_mouse_state: MouseState::new(),
            quit: false,
        }
    }
}

#[cfg(windows)]
fn detect_mouse_button_release_outside_window(state: &mut State) {
    use winapi::um::winuser::GetAsyncKeyState;

    state.mouse_state.pressed.0 &= unsafe { GetAsyncKeyState(1) as u16 & 0b1000_0000_0000_0000 > 0 };
    state.mouse_state.pressed.1 &= unsafe { GetAsyncKeyState(2) as u16 & 0b1000_0000_0000_0000 > 0 };
    state.mouse_state.pressed.2 &= unsafe { GetAsyncKeyState(4) as u16 & 0b1000_0000_0000_0000 > 0 };
}

#[cfg(not(windows))]
fn detect_mouse_button_release_outside_window(_state: &mut State) {
}

fn open_file(path: &str, state: &mut State) {

    if !state.loading.compare_and_swap(false, true, Ordering::SeqCst) {

        state.data.blocks.lock().unwrap().clear();

        let blocks = state.data.blocks.clone();
        let loading = state.loading.clone();
        let owned_path = path.to_owned();

        state.loading_thread = Some(thread::spawn(move || {

            let mut t = timer::Timer::new();

            if let Ok(file) = File::open(&owned_path) {

                let reader = BufReader::new(&file);

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
            }

            println!("Load time: {}", t.reset());

            loading.store(false, Ordering::SeqCst);
        }));
    }
}

fn save_file(path: &str, state: &State) {

}

fn run(ui: &Ui, state: &mut State) {

    if state.mouse_state.pressed.0 {
        state.pan.0 += state.last_mouse_state.pos.0 as f32 - state.mouse_state.pos.0 as f32;
        state.pan.1 += state.last_mouse_state.pos.1 as f32 - state.mouse_state.pos.1 as f32;
    }

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
                            nfd::open_file_dialog(Some("txt"), None) {

                            state.pan = (0.0, 0.0);
                            open_file(&path, state);
                        }
                    }
                    if ui.menu_item(im_str!("Save"))
                        .enabled(!state.loading.load(Ordering::SeqCst))
                        .build() {

                        if let Ok(nfd::Response::Okay(path)) =
                            nfd::open_save_dialog(Some("txt"), None) {
                                
                            save_file(&path, state);
                        }
                    }

                    if ui.menu_item(im_str!("Close"))
                        .enabled(!state.loading.load(Ordering::SeqCst) &&
                            state.data.blocks.lock().unwrap().len() > 0)
                        .build() {

                        state.pan = (0.0, 0.0);
                        state.data.blocks.lock().unwrap().clear();
                    }
                });
            });

            ui.with_window_draw_list(|d| {

                let blocks = state.data.blocks.lock().unwrap();

                let width = ui.imgui().display_size().0 as i32;

                {
                    let capacity = state.data.points.capacity();
                    state.data.points.clear();
                    state.data.points.reserve_exact(max(capacity as i32 - width, 0) as usize);
                }

                for x in 0..width {

                    let x_lookup = x + state.pan.0 as i32;

                    if let Some(block) = blocks.get((x / 4096) as usize) {

                        if let Some(value) = block.data.get((x_lookup % 4096) as usize) {

                            state.data.points.push(ImVec2::new(
                                x as f32,
                                (400.0 + 5.0*value) as f32 - state.pan.1 as f32));
                        }
                    }
                }

                d.add_poly_line(
                            &state.data.points,
                            0xdf00dfff,
                            false,
                            1.0,
                            true);
            });

            ui.text(im_str!("Fps: {:.1} {:.2} ms", ui.framerate(), 1000.0 / ui.framerate()));

            ui.text(im_str!("{:#?}", state));
        });
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
    
    let mut renderer = Renderer::init(&mut imgui, &display)
        .expect("Failed to initialize renderer");

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

    let mut s = State::new();

    loop {

        s.last_mouse_state = s.mouse_state;
        s.mouse_state.wheel = 0.0;

        let mut new_absolute_mouse_pos = None;

        events_loop.poll_events(|event| {
            use glium::glutin::{
                DeviceEvent,
                ElementState,
                Event,
                MouseButton,
                MouseScrollDelta,
                TouchPhase,
                WindowEvent,
            };

            match event {

                Event::DeviceEvent { event, .. } => {
                    match event {
                        DeviceEvent::MouseMotion { delta: (x, y), .. } => {
                            s.mouse_state.pos.0 += x as i32;
                            s.mouse_state.pos.1 += y as i32;
                        },
                        _ => (),
                    }
                },

                Event::WindowEvent { event, .. } => {
                    match event {
                        WindowEvent::Closed => {
                            s.quit = true;
                        },
                        WindowEvent::Resized(w, h) => {
                            display.gl_window().resize(w, h);
                        },
                        WindowEvent::KeyboardInput { input, .. } => {
                            use glium::glutin::VirtualKeyCode as Key;

                            let pressed = input.state == ElementState::Pressed;
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
                        },
                        WindowEvent::CursorMoved { position: (x, y), .. } => {
                            new_absolute_mouse_pos = Some((x as i32, y as i32));
                        },
                        WindowEvent::MouseInput { state, button, .. } => {
                            match button {
                                MouseButton::Left => s.mouse_state.pressed.0 = state == ElementState::Pressed,
                                MouseButton::Right => s.mouse_state.pressed.1 = state == ElementState::Pressed,
                                MouseButton::Middle => s.mouse_state.pressed.2 = state == ElementState::Pressed,
                                _ => {}
                            }
                        },
                        WindowEvent::MouseWheel {
                            delta: MouseScrollDelta::LineDelta(_, y),
                            phase: TouchPhase::Moved,
                            ..
                        } |
                        WindowEvent::MouseWheel {
                            delta: MouseScrollDelta::PixelDelta(_, y),
                            phase: TouchPhase::Moved,
                            ..
                        } => {
                            s.mouse_state.wheel = y;
                        },
                        WindowEvent::ReceivedCharacter(c) => {
                            imgui.add_input_character(c)
                        },
                        _ => (),
                    }
                },
                _ => (),
            }
        });

        if let Some(pos) = new_absolute_mouse_pos {
            s.mouse_state.pos = pos;
        }

        detect_mouse_button_release_outside_window(&mut s);

        {
            let scale = imgui.display_framebuffer_scale();

            imgui.set_mouse_pos(
                s.mouse_state.pos.0 as f32 / scale.0,
                s.mouse_state.pos.1 as f32 / scale.1,
            );

            imgui.set_mouse_down(
                &[
                    s.mouse_state.pressed.0,
                    s.mouse_state.pressed.1,
                    s.mouse_state.pressed.2,
                    false,
                    false,
                ],
            );

            imgui.set_mouse_wheel(s.mouse_state.wheel / scale.1);
        }

        let gl_window = display.gl_window();
        let size_pixels = gl_window.get_inner_size().unwrap();
        let size_points = {
            let hidpi = gl_window.hidpi_factor();
            ((size_pixels.0 as f32 / hidpi) as u32, (size_pixels.1 as f32 / hidpi) as u32)
        };


        {
            let ui = imgui.frame(size_points, size_pixels, s.frame_timer.reset() as f32);
            run(&ui, &mut s);
        
            let mut target = display.draw();
            target.clear_color(0.35, 0.3, 0.3, 1.0);
            renderer.render(&mut target, ui).expect("Rendering failed");
            target.finish().unwrap();
        }

        if s.quit {
            break;
        }
    }    
}
