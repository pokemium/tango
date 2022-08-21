use crate::{audio, battle, font, input, ipc, session, stats, video};
use ab_glyph::{Font, ScaleFont};
use parking_lot::Mutex;
use std::sync::Arc;

pub const EXPECTED_FPS: f32 = 60.0;

pub fn run(
    rt: tokio::runtime::Runtime,
    ipc_sender: Arc<Mutex<ipc::Sender>>,
    window_title: String,
    input_mapping: input::Mapping,
    rom_path: std::path::PathBuf,
    save_path: std::path::PathBuf,
    window_scale: u32,
    video_filter: Box<dyn video::Filter>,
    match_init: Option<battle::MatchInit>,
) -> Result<(), anyhow::Error> {
    let handle = rt.handle().clone();

    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let game_controller = sdl.game_controller().unwrap();
    let audio = sdl.audio().unwrap();

    let title_prefix = format!("Tango: {}", window_title);

    let window = video
        .window(
            &title_prefix,
            mgba::gba::SCREEN_WIDTH * window_scale,
            mgba::gba::SCREEN_HEIGHT * window_scale,
        )
        .opengl()
        .resizable()
        .build()
        .unwrap();

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .unwrap();

    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(
            sdl2::pixels::PixelFormatEnum::ABGR8888,
            mgba::gba::SCREEN_WIDTH as u32,
            mgba::gba::SCREEN_HEIGHT as u32,
        )
        .unwrap();

    let audio_cb = audio::LateBinder::<i16>::new();
    let audio_device = audio
        .open_playback(
            None,
            &sdl2::audio::AudioSpecDesired {
                freq: Some(48000),
                channels: Some(audio::NUM_CHANNELS as u8),
                samples: Some(512),
            },
            {
                let audio_cb = audio_cb.clone();
                |_| audio_cb
            },
        )
        .unwrap();
    log::info!("audio spec: {:?}", audio_device.spec());
    audio_device.resume();

    let fps_counter = Arc::new(Mutex::new(stats::Counter::new(30)));
    let emu_tps_counter = Arc::new(Mutex::new(stats::Counter::new(10)));

    let mut input_state = input_helper::State::new();

    let mut controllers: std::collections::HashMap<u32, sdl2::controller::GameController> =
        std::collections::HashMap::new();
    // Preemptively enumerate controllers.
    for which in 0..game_controller.num_joysticks().unwrap() {
        if !game_controller.is_game_controller(which) {
            continue;
        }
        let controller = game_controller.open(which).unwrap();
        log::info!("controller added: {}", controller.name());
        controllers.insert(which, controller);
    }

    let font = ab_glyph::FontRef::try_from_slice(&include_bytes!("fonts/FSEX300.ttf")[..]).unwrap();
    let scale = ab_glyph::PxScale::from(16.0);
    let scaled_font = font.as_scaled(scale);

    {
        let mut current_session = Some(session::Session::new(
            rt.handle().clone(),
            ipc_sender.clone(),
            audio_cb.clone(),
            audio_device.spec().clone(),
            rom_path,
            save_path,
            emu_tps_counter.clone(),
            match_init,
        )?);

        let mut event_loop = sdl.event_pump().unwrap();

        rt.block_on(async {
            ipc_sender
                .lock()
                .send(ipc::protos::FromCoreMessage {
                    which: Some(ipc::protos::from_core_message::Which::StateEv(
                        ipc::protos::from_core_message::StateEvent {
                            state: ipc::protos::from_core_message::state_event::State::Running
                                .into(),
                        },
                    )),
                })
                .await?;
            anyhow::Result::<()>::Ok(())
        })?;

        let mut show_debug_pressed = false;
        let mut show_debug = false;

        'toplevel: loop {
            // Handle events.
            for event in event_loop.poll_iter() {
                match event {
                    sdl2::event::Event::Quit { .. } => {
                        break 'toplevel;
                    }
                    sdl2::event::Event::KeyDown {
                        scancode: Some(scancode),
                        repeat: false,
                        ..
                    } => {
                        input_state.handle_key_down(scancode as usize);
                    }
                    sdl2::event::Event::KeyUp {
                        scancode: Some(scancode),
                        repeat: false,
                        ..
                    } => {
                        input_state.handle_key_up(scancode as usize);
                    }
                    sdl2::event::Event::ControllerDeviceAdded { which, .. } => {
                        if !game_controller.is_game_controller(which) {
                            continue;
                        }
                        let controller = game_controller.open(which).unwrap();
                        log::info!("controller added: {}", controller.name());
                        controllers.insert(which, controller);
                        input_state.handle_controller_connected(
                            which,
                            sdl2::sys::SDL_GameControllerAxis::SDL_CONTROLLER_AXIS_MAX as usize,
                        );
                    }
                    sdl2::event::Event::ControllerDeviceRemoved { which, .. } => {
                        if let Some(controller) = controllers.remove(&which) {
                            log::info!("controller removed: {}", controller.name());
                            input_state.handle_controller_disconnected(which);
                        }
                    }
                    sdl2::event::Event::ControllerAxisMotion {
                        axis, value, which, ..
                    } => {
                        input_state.handle_controller_axis_motion(which, axis as usize, value);
                    }
                    sdl2::event::Event::ControllerButtonDown { button, which, .. } => {
                        input_state.handle_controller_button_down(which, button as usize);
                    }
                    sdl2::event::Event::ControllerButtonUp { button, which, .. } => {
                        input_state.handle_controller_button_up(which, button as usize);
                    }
                    _ => {}
                }

                let last_show_debug_pressed = show_debug_pressed;
                show_debug_pressed =
                    input_state.is_key_pressed(sdl2::keyboard::Scancode::Grave as usize);
                if show_debug_pressed && !last_show_debug_pressed {
                    show_debug = !show_debug;
                }

                if let Some(session) = current_session.as_ref() {
                    session.set_joyflags(input_mapping.to_mgba_keys(&input_state));
                }
            }

            canvas.clear();

            let is_session_active = current_session
                .as_ref()
                .map(|s| {
                    s.match_()
                        .as_ref()
                        .map(|match_| handle.block_on(async { match_.lock().await.is_some() }))
                        .unwrap_or(true)
                })
                .unwrap_or(false);
            if !is_session_active {
                current_session = None;
            }

            if current_session.is_none() {
                break 'toplevel;
            }

            if let Some(session) = &current_session {
                // If we're in single-player mode, allow speedup.
                if session.match_().is_none() {
                    session.set_fps(
                        if input_mapping
                            .speed_up
                            .iter()
                            .any(|c| c.is_active(&input_state))
                        {
                            EXPECTED_FPS * 3.0
                        } else {
                            EXPECTED_FPS
                        },
                    );
                }

                // If we've crashed, log the error and panic.
                if let Some(thread_handle) = session.has_crashed() {
                    // HACK: No better way to lock the core.
                    let audio_guard = thread_handle.lock_audio();
                    panic!(
                        "mgba thread crashed!\nlr = {:08x}, pc = {:08x}",
                        audio_guard.core().gba().cpu().gpr(14),
                        audio_guard.core().gba().cpu().thumb_pc()
                    );
                }

                // Apply stupid video scaling filter that only mint wants 🥴
                let (vbuf_width, vbuf_height) = video_filter.output_size((
                    mgba::gba::SCREEN_WIDTH as usize,
                    mgba::gba::SCREEN_HEIGHT as usize,
                ));

                let tq = texture.query();
                if tq.width != vbuf_width as u32 || tq.height != vbuf_height as u32 {
                    log::info!(
                        "texture reallocated: ({}, {}) -> ({}, {})",
                        tq.width,
                        tq.height,
                        vbuf_width,
                        vbuf_height
                    );
                    texture = texture_creator
                        .create_texture_streaming(
                            sdl2::pixels::PixelFormatEnum::ABGR8888,
                            vbuf_width as u32,
                            vbuf_height as u32,
                        )
                        .unwrap();
                }
                texture
                    .with_lock(
                        sdl2::rect::Rect::new(0, 0, vbuf_width as u32, vbuf_height as u32),
                        |buf, _pitch| {
                            video_filter.apply(
                                &session.lock_vbuf(),
                                buf,
                                (
                                    mgba::gba::SCREEN_WIDTH as usize,
                                    mgba::gba::SCREEN_HEIGHT as usize,
                                ),
                            );
                        },
                    )
                    .unwrap();

                let viewport = canvas.viewport();
                let scaling_factor = std::cmp::max(
                    std::cmp::min(
                        viewport.width() / mgba::gba::SCREEN_WIDTH,
                        viewport.height() / mgba::gba::SCREEN_HEIGHT,
                    ),
                    1,
                );

                let (new_width, new_height) = (
                    (mgba::gba::SCREEN_WIDTH * scaling_factor) as u32,
                    (mgba::gba::SCREEN_HEIGHT * scaling_factor) as u32,
                );
                canvas
                    .copy(
                        &texture,
                        None,
                        sdl2::rect::Rect::new(
                            viewport.x() + (viewport.width() as i32 - new_width as i32) / 2,
                            viewport.y() + (viewport.height() as i32 - new_height as i32) / 2,
                            new_width,
                            new_height,
                        ),
                    )
                    .unwrap();

                // Update title to show P1/P2 state.
                let mut title = title_prefix.to_string();
                if let Some(match_) = session.match_().as_ref() {
                    rt.block_on(async {
                        if let Some(match_) = &*match_.lock().await {
                            let round_state = match_.lock_round_state().await;
                            if let Some(round) = round_state.round.as_ref() {
                                title = format!("{} [P{}]", title, round.local_player_index() + 1);
                            }
                        }
                    });
                }
                canvas.window_mut().set_title(&title).unwrap();

                // TODO: Figure out why moving this into its own function locks fps to tps.
                if show_debug {
                    let mut lines = vec![format!(
                        "fps: {:3.02}",
                        1.0 / fps_counter.lock().mean_duration().as_secs_f32()
                    )];

                    let tps_adjustment = if let Some(match_) = session.match_().as_ref() {
                        handle.block_on(async {
                            if let Some(match_) = &*match_.lock().await {
                                lines.push("match active".to_string());
                                let round_state = match_.lock_round_state().await;
                                if let Some(round) = round_state.round.as_ref() {
                                    lines.push(format!("current tick: {:4}", round.current_tick()));
                                    lines.push(format!(
                                        "local player index: {}",
                                        round.local_player_index()
                                    ));
                                    lines.push(format!(
                                        "qlen: {:2} vs {:2} (delay = {:1})",
                                        round.local_queue_length(),
                                        round.remote_queue_length(),
                                        round.local_delay(),
                                    ));
                                    round.tps_adjustment()
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            }
                        })
                    } else {
                        0.0
                    };

                    lines.push(format!(
                        "emu tps: {:3.02} ({:+1.02})",
                        1.0 / emu_tps_counter.lock().mean_duration().as_secs_f32(),
                        tps_adjustment
                    ));

                    for (i, line) in lines.iter().enumerate() {
                        let mut glyphs = Vec::new();
                        font::layout_paragraph(
                            scaled_font,
                            ab_glyph::point(0.0, 0.0),
                            9999.0,
                            &line,
                            &mut glyphs,
                        );

                        let height = scaled_font.height().ceil() as i32;
                        let width = {
                            let min_x = glyphs.first().unwrap().position.x;
                            let last_glyph = glyphs.last().unwrap();
                            let max_x =
                                last_glyph.position.x + scaled_font.h_advance(last_glyph.id);
                            (max_x - min_x).ceil() as i32
                        };

                        let mut texture = texture_creator
                            .create_texture_streaming(
                                sdl2::pixels::PixelFormatEnum::ABGR8888,
                                width as u32,
                                height as u32,
                            )
                            .unwrap();
                        texture
                            .with_lock(
                                sdl2::rect::Rect::new(0, 0, width as u32, height as u32),
                                |buf, _pitch| {
                                    for glyph in glyphs {
                                        if let Some(outlined) = scaled_font.outline_glyph(glyph) {
                                            let bounds = outlined.px_bounds();
                                            outlined.draw(|x, y, v| {
                                                let x = x as i32 + bounds.min.x as i32;
                                                let y = y as i32 + bounds.min.y as i32;
                                                if x >= width || y >= height || x < 0 || y < 0 {
                                                    return;
                                                }
                                                let gray = (v * 0xff as f32) as u8;
                                                buf[((y * width + x) * 4) as usize + 0] = gray;
                                                buf[((y * width + x) * 4) as usize + 1] = gray;
                                                buf[((y * width + x) * 4) as usize + 2] = gray;
                                                buf[((y * width + x) * 4) as usize + 3] = 0xff;
                                            });
                                        }
                                    }
                                },
                            )
                            .unwrap();

                        canvas
                            .copy(
                                &texture,
                                None,
                                Some(sdl2::rect::Rect::new(
                                    0,
                                    (i * height as usize) as i32,
                                    width as u32,
                                    height as u32,
                                )),
                            )
                            .unwrap();
                    }
                }
            }

            // Done!
            canvas.present();
            fps_counter.lock().mark();
        }
    }

    log::info!("goodbye");
    Ok(())
}
