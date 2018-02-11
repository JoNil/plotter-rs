extern crate arrayvec;

extern crate glium;

#[macro_use]
extern crate imgui;
extern crate imgui_glium_renderer;

extern crate nfd;

extern crate time;

#[cfg(windows)]
extern crate winapi;

// Use https://github.com/tafia/quick-csv for csv

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
    data0: arrayvec::ArrayVec<[f64; 4096]>,
    data1: arrayvec::ArrayVec<[f64; 2048]>,
    data2: arrayvec::ArrayVec<[f64; 1024]>,
    data3: arrayvec::ArrayVec<[f64; 512]>,
    data4: arrayvec::ArrayVec<[f64; 256]>,
    data5: arrayvec::ArrayVec<[f64; 128]>,
    data6: arrayvec::ArrayVec<[f64; 64]>,
    data7: arrayvec::ArrayVec<[f64; 32]>,
    data8: arrayvec::ArrayVec<[f64; 16]>,
    data9: arrayvec::ArrayVec<[f64; 8]>,
}

impl Block {
    fn new() -> Block {
        Block {
            data0: arrayvec::ArrayVec::new(),
            data1: arrayvec::ArrayVec::new(),
            data2: arrayvec::ArrayVec::new(),
            data3: arrayvec::ArrayVec::new(),
            data4: arrayvec::ArrayVec::new(),
            data5: arrayvec::ArrayVec::new(),
            data6: arrayvec::ArrayVec::new(),
            data7: arrayvec::ArrayVec::new(),
            data8: arrayvec::ArrayVec::new(),
            data9: arrayvec::ArrayVec::new(),
        }
    }

    fn push(&mut self, val: f64) {
        self.data0.push(val);
    }

    fn lookup(&self, x: f64, zoom: f64) -> Option<f64> {

        self.data0.get((x as i32 % 4096) as usize).map(|p| *p)

    }
}

trait Lookup {
    fn lookup(&self, x: f64, zoom: f64) -> Option<f64>;
}

impl Lookup for [Block] {

    fn lookup(&self, x: f64, zoom: f64) -> Option<f64> {

        self.get((x as i32 / 4096) as usize).and_then(|block| block.lookup(x, zoom))
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
    stop_loading: Arc<AtomicBool>,
    loading_thread: Option<thread::JoinHandle<()>>,

    data: Data,

    pan: (f64, f64),

    frame_timer: timer::Timer,

    mouse_state: MouseState,
    last_mouse_state: MouseState,

    quit: bool,

    scroll_factor: f64,
}

impl State {
    fn new() -> State {
        State {
            loading: Arc::new(AtomicBool::new(false)),
            stop_loading: Arc::new(AtomicBool::new(false)),
            loading_thread: None,
            data: Data::new(),
            pan: (0.0, 0.0),
            frame_timer: timer::Timer::new(),
            mouse_state: MouseState::new(),
            last_mouse_state: MouseState::new(),
            quit: false,
            scroll_factor: 0.0,
        }
    }
}

fn open_file(path: &str, state: &mut State) {

    if !state.loading.compare_and_swap(false, true, Ordering::SeqCst) {

        state.stop_loading.store(false, Ordering::SeqCst);

        state.data.blocks.lock().unwrap().clear();

        let blocks = state.data.blocks.clone();
        let loading = state.loading.clone();
        let stop_loading = state.stop_loading.clone();
        let owned_path = path.to_owned();

        state.loading_thread = Some(thread::spawn(move || {

            let mut t = timer::Timer::new();

            if let Ok(file) = File::open(&owned_path) {

                let reader = BufReader::new(&file);

                    let mut block = Block::new();

                    for maybe_line in reader.lines() {

                        if stop_loading.load(Ordering::SeqCst) { 
                            break;
                        }

                        if let Ok(line) = maybe_line {
                            if let Ok(val) = line.parse::<f64>() {

                                if block.data0.is_full() {
                                    blocks.lock().unwrap().push(block);
                                    block = Block::new();
                                }

                                block.push(val);
                            }
                        }
                    }

                    if block.data0.len() != 0 {
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

    let view_size = ui.imgui().display_size();

    if state.mouse_state.pressed.0 {
        state.pan.0 += state.last_mouse_state.pos.0 as f64 - state.mouse_state.pos.0 as f64;
        state.pan.1 -= state.last_mouse_state.pos.1 as f64 - state.mouse_state.pos.1 as f64;
    }

    if state.mouse_state.wheel != 0.0 {
        let mouse_centered_x = state.mouse_state.pos.0 as f64 - view_size.0 as f64 / 2.0;

        let new_scroll_factor = state.scroll_factor - state.mouse_state.wheel as f64 / 10.0;

        let last_scale = f64::exp(state.scroll_factor);
        let new_scale = f64::exp(new_scroll_factor);

        let mouse_centered_last_scale_x = (state.pan.0 + mouse_centered_x) / last_scale;
        let mouse_centered_scale_x = (state.pan.0 + mouse_centered_x) / new_scale;
    
        state.pan.0 -= (mouse_centered_last_scale_x - mouse_centered_scale_x) * last_scale;

        state.scroll_factor = new_scroll_factor;
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
                        .enabled(state.data.blocks.lock().unwrap().len() > 0)
                        .build() {

                        state.pan = (0.0, 0.0);
                        state.scroll_factor = 0.0;
                        state.stop_loading.store(true, Ordering::SeqCst);

                        while state.loading.load(Ordering::SeqCst) {
                            thread::sleep(std::time::Duration::from_millis(1));
                        }

                        state.data.blocks.lock().unwrap().clear();
                    }
                });
            });

            ui.with_window_draw_list(|d| {

                let blocks = state.data.blocks.lock().unwrap();

                let scale = f64::exp(state.scroll_factor as f64);

                ui.text(im_str!("Zoom {:?}", scale));

                {
                    let capacity = state.data.points.capacity();
                    state.data.points.clear();
                    state.data.points.reserve_exact(max(capacity as i32 - view_size.0 as i32, 0) as usize);
                }

                for x in 0..(view_size.0 as i32) {

                    let x_lookup = scale*(x as f64 + state.pan.0 - view_size.0 as f64 / 2.0);

                    if let Some(value) = blocks.lookup(x_lookup, scale) {

                        state.data.points.push(ImVec2::new(
                            x as f32,
                            (10.0*value + state.pan.1 + view_size.1 as f64 / 2.0) as f32));
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
