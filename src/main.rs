extern crate arrayvec;
extern crate byteorder;
extern crate clamp;
extern crate glium;
extern crate nfd;
extern crate serialport;
extern crate time;
extern crate image;

#[macro_use]
extern crate imgui;
extern crate imgui_glium_renderer;

#[cfg(windows)]
extern crate winapi;

use byteorder::{ByteOrder, LittleEndian};
use glium::glutin::{
    dpi::LogicalPosition, dpi::LogicalSize, Api, ContextBuilder, EventsLoop, GlContext, GlProfile,
    GlRequest, WindowBuilder, Icon,
};
use glium::{Display, Surface};
use imgui::{FrameSize, ImGui, ImGuiCond, ImGuiKey, ImVec2, StyleVar, Ui};
use imgui_glium_renderer::Renderer;
use std::cmp::max;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use clamp::clamp;

mod timer;

struct Block {
    data0: arrayvec::ArrayVec<[f64; 32]>,
}

impl Block {
    fn new() -> Block {
        Block {
            data0: arrayvec::ArrayVec::new(),
        }
    }

    fn push(&mut self, val: f64) {
        self.data0.push(val);
    }

    fn lookup(&self, x: f64, _zoom: f64) -> Option<f64> {
        self.data0.get((x as i32 % 32) as usize).map(|p| *p)
    }
}

trait Lookup {
    fn lookup(&self, x: f64, zoom: f64) -> Option<f64>;
}

impl Lookup for [Box<Block>] {
    fn lookup(&self, x: f64, zoom: f64) -> Option<f64> {
        self.get((x as i32 / 32) as usize)
            .and_then(|block| block.lookup(x, zoom))
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
    blocks_ch0: Arc<Mutex<Vec<Box<Block>>>>,
    blocks_ch1: Arc<Mutex<Vec<Box<Block>>>>,
    points_ch0: Vec<ImVec2>,
    points_ch1: Vec<ImVec2>,
}

impl Data {
    fn new() -> Data {
        Data {
            blocks_ch0: Arc::new(Mutex::new(Vec::new())),
            blocks_ch1: Arc::new(Mutex::new(Vec::new())),
            points_ch0: Vec::new(),
            points_ch1: Vec::new(),
        }
    }
}

impl fmt::Debug for Data {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Data {{ blocks_ch0: {}, blocks_ch1: {}, points_ch0: {}, points_ch1: {} }}",
            self.blocks_ch0.lock().unwrap().len(),
            self.blocks_ch1.lock().unwrap().len(),
            self.points_ch0.len(),
            self.points_ch1.len()
        )
    }
}

#[derive(Debug)]
struct State {
    loading: Arc<AtomicBool>,
    stop_loading: Arc<AtomicBool>,
    loading_thread: Option<thread::JoinHandle<()>>,

    data: Data,

    pan: (f64, f64),
    panning: bool,

    frame_timer: timer::Timer,

    mouse_state: MouseState,
    last_mouse_state: MouseState,

    quit: bool,

    scroll_factor: f64,

    window_y_scale: f32,

    ch0_pan: f32,
    ch0_scale: f32,
    ch0_smooth: Arc<Mutex<f32>>,
    
    ch1_pan: f32,
    ch1_scale: f32,

    rise_value: Arc<Mutex<f32>>,
}

impl State {
    fn new() -> State {
        State {
            loading: Arc::new(AtomicBool::new(false)),
            stop_loading: Arc::new(AtomicBool::new(false)),
            loading_thread: None,
            data: Data::new(),
            pan: (0.0, 0.0),
            panning: false,
            frame_timer: timer::Timer::new(),
            mouse_state: MouseState::new(),
            last_mouse_state: MouseState::new(),
            quit: false,
            scroll_factor: 0.0,
            window_y_scale: 1.0,
            ch0_pan: 1.0,
            ch0_scale: 1.0,
            ch0_smooth: Arc::new(Mutex::new(0.0)),
            ch1_pan: 1.0,
            ch1_scale: 1.0,
            rise_value: Arc::new(Mutex::new(0.0)),
        }
    }
}

fn open_file(path: &str, state: &mut State) {
    if !state
        .loading
        .compare_and_swap(false, true, Ordering::SeqCst)
    {
        state.stop_loading.store(false, Ordering::SeqCst);

        state.data.blocks_ch0.lock().unwrap().clear();

        let blocks = state.data.blocks_ch0.clone();
        let loading = state.loading.clone();
        let stop_loading = state.stop_loading.clone();
        let owned_path = path.to_owned();

        state.loading_thread = Some(thread::spawn(move || {
            let mut t = timer::Timer::new();

            if let Ok(file) = File::open(&owned_path) {
                let reader = BufReader::new(&file);

                let mut block = Box::new(Block::new());

                for maybe_line in reader.lines() {
                    if stop_loading.load(Ordering::SeqCst) {
                        break;
                    }

                    if let Ok(line) = maybe_line {
                        if let Ok(val) = line.parse::<f64>() {
                            if block.data0.is_full() {
                                blocks.lock().unwrap().push(block);
                                block = Box::new(Block::new());
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

fn open_com_port(state: &mut State) {
    if !state
        .loading
        .compare_and_swap(false, true, Ordering::SeqCst)
    {
        state.stop_loading.store(false, Ordering::SeqCst);

        state.data.blocks_ch0.lock().unwrap().clear();
        state.data.blocks_ch1.lock().unwrap().clear();

        let blocks_ch0 = state.data.blocks_ch0.clone();
        let blocks_ch1 = state.data.blocks_ch1.clone();
        let loading = state.loading.clone();
        let stop_loading = state.stop_loading.clone();

        let ch0_smooth = state.ch0_smooth.clone();
        let rise_value = state.rise_value.clone();

        state.loading_thread = Some(thread::spawn(move || {
            let s = serialport::SerialPortSettings {
                baud_rate: 250000,
                data_bits: serialport::DataBits::Eight,
                flow_control: serialport::FlowControl::None,
                parity: serialport::Parity::None,
                stop_bits: serialport::StopBits::One,
                timeout: Duration::from_millis(1),
            };

            let mut sp = {
                let mut sp = None;

                for i in 1..=30 {
                    let port = format!("COM{}", i);

                    match serialport::open_with_settings(&port, &s) {
                        Ok(s) => {
                            println!("Using {}", port);
                            sp = Some(s);
                            break;
                        }
                        Err(_) => {}
                    }
                }

                if let Some(s) = sp {
                    s
                } else {
                    loading.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let mut buffer = Vec::new();

            let mut block_ch0 = Box::new(Block::new());
            let mut block_ch1 = Box::new(Block::new());

            let mut ch0_avg = 0.0;

            let mut start_timestamp = 0;
            let mut last_ligh_on = 0;
            let mut measuring_cycle = false;

            while !stop_loading.load(Ordering::SeqCst) {
                
                {
                    let mut receive_buffer = [0; 6];

                    match sp.read(&mut receive_buffer) {
                        Ok(amt) => {
                            buffer.extend_from_slice(&receive_buffer[..amt]);
                        }
                        Err(ref e) if e.kind() == ErrorKind::TimedOut => (),
                        Err(e) => println!("{:?}", e),
                    }
                }

                if buffer.len() > 6 {

                    let value = LittleEndian::read_u16(&buffer[..2]);
                    
                    let sync = (value >> 15) & 0b1;

                    if sync != 0b1 {
                        println!("OUT OF SYNC");
                        buffer.remove(0);
                    } else {

                        let light_on = (value >> 14) & 0b1;

                        let high = (value >> 8) & 0b00011111;
                        let low = value & 0b00011111;

                        let analog = (high << 5) | low;

                        let analog_flipped = 1024.0 - analog as f64;

                        let ch0_smooth_value = *ch0_smooth.lock().unwrap() as f64;

                        ch0_avg = ch0_avg*ch0_smooth_value + analog_flipped*(1.0 - ch0_smooth_value);

                        let time = {
                            let time_packed = LittleEndian::read_u32(&buffer[2..6]);

                            // The high bit in every byte of time_packed is 0 becouse of the sync bit 
                            // in the high byte of value above. So we need to unpack this into a proper u32.
                            // The bottom 4 bit of the u32 is discarded on the arduino to make room.

                            let time = 
                                ( ((time_packed >> 3) & 0x0fe0_0000)
                                | ((time_packed >> 2) & 0x001f_c000)
                                | ((time_packed >> 1) & 0x0000_3f80)
                                | ((time_packed)      & 0x0000_007f)) << 4;

                            time
                        };

                        {
                            if last_ligh_on == 1 && light_on == 0 {
                                start_timestamp = time;
                                measuring_cycle = true;
                            }

                            if measuring_cycle && light_on == 1 {
                                measuring_cycle = false;
                            }

                            if measuring_cycle && ch0_avg < *rise_value.lock().unwrap() as f64 {
                                measuring_cycle = false;
                                let latency = time - start_timestamp;
                                println!("{}", latency as f64 / 1000.0);
                            }

                            last_ligh_on = light_on;
                        }

                        {
                            if block_ch0.data0.is_full() {
                                blocks_ch0.lock().unwrap().push(block_ch0);
                                block_ch0 = Box::new(Block::new());
                            }

                            if block_ch1.data0.is_full() {
                                blocks_ch1.lock().unwrap().push(block_ch1);
                                block_ch1 = Box::new(Block::new());
                            }

                            block_ch0.push(ch0_avg);
                            block_ch1.push(if light_on == 1 { 100.0 } else { 0.0 });
                        }

                        buffer.remove(0);
                        buffer.remove(0);
                        buffer.remove(0);
                        buffer.remove(0);
                        buffer.remove(0);
                        buffer.remove(0);
                    }
                }
            }

            loading.store(false, Ordering::SeqCst);
        }));
    }
}

fn run(ui: &Ui, state: &mut State) {
    let view_size = ui.imgui().display_size();

    ui.window(im_str!("Main"))
        .size(ui.imgui().display_size(), ImGuiCond::Always)
        .position((0.0, 0.0), ImGuiCond::Always)
        .movable(false)
        .resizable(false)
        .title_bar(false)
        .collapsible(false)
        .menu_bar(true)
        .no_bring_to_front_on_focus(true)
        .build(|| {
            ui.menu_bar(|| {
                ui.menu(im_str!("File")).build(|| {
                    if ui
                        .menu_item(im_str!("Open"))
                        .enabled(!state.loading.load(Ordering::SeqCst))
                        .build()
                    {
                        if let Ok(nfd::Response::Okay(path)) =
                            nfd::open_file_dialog(Some("txt"), None)
                        {
                            state.pan = (0.0, 0.0);
                            open_file(&path, state);
                        }
                    }

                    if ui
                        .menu_item(im_str!("Open COM port"))
                        .enabled(!state.loading.load(Ordering::SeqCst))
                        .build()
                    {
                        open_com_port(state);
                    }

                    if ui
                        .menu_item(im_str!("Close"))
                        .enabled(state.loading.load(Ordering::SeqCst))
                        .build()
                    {
                        state.pan = (0.0, 0.0);
                        state.scroll_factor = 0.0;
                        state.stop_loading.store(true, Ordering::SeqCst);

                        while state.loading.load(Ordering::SeqCst) {
                            thread::sleep(std::time::Duration::from_millis(1));
                        }

                        state.data.blocks_ch0.lock().unwrap().clear();
                        state.data.blocks_ch1.lock().unwrap().clear();
                    }
                });
            });

            let menu_bar_hovered = ui.is_item_hovered();

            if !menu_bar_hovered && ui.is_window_hovered() && state.mouse_state.pressed.0 {
                state.panning = true;
            }

            if !state.mouse_state.pressed.0 {
                state.panning = false;
            }

            if (!menu_bar_hovered && ui.is_window_hovered()) || state.panning {
                if state.mouse_state.pressed.0 {
                    state.pan.0 +=
                        state.last_mouse_state.pos.0 as f64 - state.mouse_state.pos.0 as f64;
                    state.pan.1 +=
                        state.last_mouse_state.pos.1 as f64 - state.mouse_state.pos.1 as f64;
                }

                if state.mouse_state.wheel != 0.0 {
                    let mouse_centered_x =
                        state.mouse_state.pos.0 as f64 - view_size.0 as f64 / 2.0;

                    let new_scroll_factor =
                        state.scroll_factor - state.mouse_state.wheel as f64 / 10.0;

                    let last_scale = f64::exp(state.scroll_factor);
                    let new_scale = f64::exp(new_scroll_factor);

                    let mouse_centered_last_scale_x = (state.pan.0 + mouse_centered_x) / last_scale;
                    let mouse_centered_scale_x = (state.pan.0 + mouse_centered_x) / new_scale;

                    state.pan.0 -=
                        (mouse_centered_last_scale_x - mouse_centered_scale_x) * last_scale;

                    state.scroll_factor = new_scroll_factor;
                }
            }

            let scale = f64::exp(state.scroll_factor as f64);

            {
                let draw_list = ui.get_window_draw_list();

                {
                    let blocks_ch0 = state.data.blocks_ch0.lock().unwrap();

                    {
                        let capacity = state.data.points_ch0.capacity();
                        state.data.points_ch0.clear();
                        state
                            .data
                            .points_ch0
                            .reserve_exact(max(capacity as i32 - view_size.0 as i32, 0) as usize);
                    }

                    for x in 0..(view_size.0 as i32) {
                        let x_lookup = scale * (x as f64 + state.pan.0 - view_size.0 as f64 / 2.0);

                        if let Some(value) = blocks_ch0.lookup(x_lookup, scale) {
                            state.data.points_ch0.push(ImVec2::new(
                                x as f32,
                                ((state.ch0_scale as f64
                                    * state.window_y_scale as f64
                                    * (value + state.ch0_pan as f64))
                                    + state.pan.1
                                    - view_size.1 as f64 / 2.0)
                                    as f32,
                            ));
                        }
                    }

                    if state.data.points_ch0.len() > 1 {
                        for (p1, p2) in state
                            .data
                            .points_ch0
                            .iter()
                            .zip(state.data.points_ch0[1..].iter())
                        {
                            draw_list
                                .add_line((p1.x, -p1.y), (p2.x, -p2.y), 0xdf00dfff)
                                .build();
                        }
                    }
                }

                {
                    let blocks_ch1 = state.data.blocks_ch1.lock().unwrap();

                    {
                        let capacity = state.data.points_ch1.capacity();
                        state.data.points_ch1.clear();
                        state
                            .data
                            .points_ch1
                            .reserve_exact(max(capacity as i32 - view_size.0 as i32, 0) as usize);
                    }

                    for x in 0..(view_size.0 as i32) {
                        let x_lookup = scale * (x as f64 + state.pan.0 - view_size.0 as f64 / 2.0);

                        if let Some(value) = blocks_ch1.lookup(x_lookup, scale) {
                            state.data.points_ch1.push(ImVec2::new(
                                x as f32,
                                ((state.ch1_scale as f64
                                    * state.window_y_scale as f64
                                    * (value + state.ch1_pan as f64))
                                    + state.pan.1
                                    - view_size.1 as f64 / 2.0)
                                    as f32,
                            ));
                        }
                    }

                    if state.data.points_ch0.len() > 1 {
                        for (p1, p2) in state
                            .data
                            .points_ch1
                            .iter()
                            .zip(state.data.points_ch1[1..].iter())
                        {
                            draw_list
                                .add_line((p1.x, -p1.y), (p2.x, -p2.y), 0xdf1010ff)
                                .build();
                        }
                    }
                }

                for i in -9..10 {
                    let x1 = 0.0;
                    let x2 = view_size.0 as f32;
                    let y = (state.pan.1 - view_size.1 as f64 / 2.0) as f32
                        + (i as f32) * state.window_y_scale * 100.0;

                    if i == 0 {
                        draw_list.add_line((x1, -y), (x2, -y), 0x602a2aff).build();
                    } else if i < 0 {
                        draw_list.add_line((x1, -y), (x2, -y), 0x2a2a2aff).build();
                    } else {
                        draw_list.add_line((x1, -y), (x2, -y), 0x2a2a2aff).build();
                    }
                }

                {
                    let x1 = 0.0;
                    let x2 = view_size.0 as f32;
                    let rise_y = (state.pan.1 - view_size.1 as f64 / 2.0) as f32
                        + state.window_y_scale * *state.rise_value.lock().unwrap();

                    draw_list
                        .add_line((x1, -rise_y), (x2, -rise_y), 0xa0ff2a2a)
                        .build();
                }
            }

            ui.with_style_vars(
                &[StyleVar::FrameRounding(3.0), StyleVar::WindowRounding(3.0)],
                || {
                    ui.window(im_str!("Properties"))
                        .size((400.0, 600.0), ImGuiCond::FirstUseEver)
                        .position((25.0, 50.0), ImGuiCond::FirstUseEver)
                        .movable(true)
                        .resizable(true)
                        .title_bar(true)
                        .collapsible(true)
                        .build(|| {
                            ui.drag_float(im_str!("Ch 0 pan"), &mut state.ch0_pan)
                                .speed(0.1)
                                .build();
                            ui.drag_float(im_str!("Ch 0 scale"), &mut state.ch0_scale)
                                .speed(0.001)
                                .build();
                            if ui.drag_float(im_str!("Ch 0 smooth"), &mut state.ch0_smooth.lock().unwrap())
                                .speed(0.001)
                                .min(0.0)
                                .max(1.0)
                                .build() {
                                    let mut lock = state.ch0_smooth.lock().unwrap();
                                    *lock = clamp(0.0, *lock, 1.0);
                                }

                            ui.drag_float(im_str!("Ch 1 pan"), &mut state.ch1_pan)
                                .speed(0.1)
                                .build();
                            ui.drag_float(im_str!("Ch 1 scale"), &mut state.ch1_scale)
                                .speed(0.001)
                                .build();

                            ui.drag_float(im_str!("Window Y Scale"), &mut state.window_y_scale)
                                .speed(0.001)
                                .build();

                            ui.drag_float(im_str!("Rise Value"), &mut state.rise_value.lock().unwrap())
                                .speed(0.1)
                                .build();

                            ui.text(im_str!(
                                "Fps: {:.1} {:.2} ms",
                                ui.framerate(),
                                1000.0 / ui.framerate()
                            ));
                            ui.text(im_str!("Zoom {:?}", scale));

                            ui.text(im_str!("{:#?}", state));
                        });
                },
            );
        });
}

#[cfg(windows)]
fn detect_mouse_button_release_outside_window(state: &mut State) {
    use winapi::um::winuser::GetAsyncKeyState;

    state.mouse_state.pressed.0 &=
        unsafe { GetAsyncKeyState(1) as u16 & 0b1000_0000_0000_0000 > 0 };
    state.mouse_state.pressed.1 &=
        unsafe { GetAsyncKeyState(2) as u16 & 0b1000_0000_0000_0000 > 0 };
    state.mouse_state.pressed.2 &=
        unsafe { GetAsyncKeyState(4) as u16 & 0b1000_0000_0000_0000 > 0 };
}

#[cfg(not(windows))]
fn detect_mouse_button_release_outside_window(_state: &mut State) {}

fn main() {

    let icon = image::open("icon/plotter-rs.png").unwrap().to_rgba();
    let (icon_w, icon_h) = (icon.width(), icon.height());

    let mut events_loop = EventsLoop::new();

    let display = {
        let context = ContextBuilder::new()
            .with_gl_profile(GlProfile::Core)
            .with_gl(GlRequest::Specific(Api::OpenGl, (4, 3)));
        let window = WindowBuilder::new()
            .with_title("plotter-rs")
            .with_dimensions((1024u32, 768u32).into())
            .with_window_icon(Some(Icon::from_rgba(icon.into_raw(), icon_w, icon_h).unwrap()));
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

    let mut s = State::new();

    let mut begin_frame;

    loop {

        begin_frame = time::precise_time_s();

        s.last_mouse_state = s.mouse_state;
        s.mouse_state.wheel = 0.0;

        let mut new_absolute_mouse_pos = None;

        events_loop.poll_events(|event| {
            use glium::glutin::{
                DeviceEvent, ElementState, Event, MouseButton, MouseScrollDelta, TouchPhase,
                WindowEvent,
            };

            match event {
                Event::DeviceEvent { event, .. } => match event {
                    DeviceEvent::MouseMotion { delta: (x, y), .. } => {
                        s.mouse_state.pos.0 += x as i32;
                        s.mouse_state.pos.1 += y as i32;
                    }
                    _ => (),
                },

                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        s.quit = true;
                    }
                    WindowEvent::Resized(LogicalSize { width, height }) => {
                        display.gl_window().resize((width, height).into());
                    }
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
                            Some(Key::LControl) | Some(Key::RControl) => {
                                imgui.set_key_ctrl(pressed)
                            }
                            Some(Key::LShift) | Some(Key::RShift) => imgui.set_key_shift(pressed),
                            Some(Key::LAlt) | Some(Key::RAlt) => imgui.set_key_alt(pressed),
                            Some(Key::LWin) | Some(Key::RWin) => imgui.set_key_super(pressed),
                            _ => {}
                        }
                    }
                    WindowEvent::CursorMoved {
                        position: LogicalPosition { x, y },
                        ..
                    } => {
                        if x as i32 != 0 && y as i32 != 0 {
                            new_absolute_mouse_pos = Some((x as i32, y as i32));
                        }
                    }
                    WindowEvent::MouseInput { state, button, .. } => match button {
                        MouseButton::Left => {
                            s.mouse_state.pressed.0 = state == ElementState::Pressed
                        }
                        MouseButton::Right => {
                            s.mouse_state.pressed.1 = state == ElementState::Pressed
                        }
                        MouseButton::Middle => {
                            s.mouse_state.pressed.2 = state == ElementState::Pressed
                        }
                        _ => {}
                    },
                    WindowEvent::MouseWheel {
                        delta: MouseScrollDelta::LineDelta(_, y),
                        phase: TouchPhase::Moved,
                        ..
                    } => {
                        s.mouse_state.wheel = y;
                    }
                    WindowEvent::MouseWheel {
                        delta: MouseScrollDelta::PixelDelta(LogicalPosition { y, .. }),
                        phase: TouchPhase::Moved,
                        ..
                    } => {
                        s.mouse_state.wheel = y as f32;
                    }
                    WindowEvent::ReceivedCharacter(c) => imgui.add_input_character(c),
                    _ => (),
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

            imgui.set_mouse_down([
                s.mouse_state.pressed.0,
                s.mouse_state.pressed.1,
                s.mouse_state.pressed.2,
                false,
                false,
            ]);

            imgui.set_mouse_wheel(s.mouse_state.wheel / scale.1);
        }

        let gl_window = display.gl_window();
        let size_pixels = gl_window.get_inner_size().unwrap();

        {
            let ui = imgui.frame(
                FrameSize::new(
                    size_pixels.width,
                    size_pixels.height,
                    gl_window.get_hidpi_factor(),
                ),
                s.frame_timer.reset() as f32,
            );
            run(&ui, &mut s);

            let mut target = display.draw();
            target.clear_color(0.35, 0.3, 0.3, 1.0);
            renderer.render(&mut target, ui).expect("Rendering failed");
            target.finish().unwrap();
        }

        if s.quit {
            break;
        }

        while time::precise_time_s() - begin_frame < 1.0 / 120.0 {
            thread::yield_now();
        }
    }
}
