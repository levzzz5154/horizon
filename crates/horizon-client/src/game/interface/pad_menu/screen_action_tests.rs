use super::{EventLoopCtl, Game, Screen, ScreenAction, ScreenActionSource};
use crate::game::native_entity_refs::tests::{run_fresh_cases, with_client_game};
use horizon_client::motion::PlayerMotion;
use horizon_client::title::{Mode, Title};
use horizon_core::geometry::Vec3;
use mod_api::{
    ClientHost, Flow, Hooks, ModEntry, ScreenActionPreEvent, ScreenRenderPreEvent, Text,
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use winit::{
    dpi::PhysicalSize,
    event_loop::{ControlFlow, EventLoop},
    keyboard::KeyCode,
    window::Window,
};

const SIZE: [u32; 2] = [640, 480];
const FIRST: [f32; 4] = [26.0, 36.0, 96.0, 20.0];
const SECOND: [f32; 4] = [26.0, 66.0, 96.0, 20.0];

#[derive(Default)]
struct Menu {
    replace: bool,
    freeze_focus: bool,
    keep_world_list: bool,
    activated: Vec<(ScreenActionSource, usize)>,
    page: i32,
    scroll: f32,
}

struct Probe(Arc<Mutex<Menu>>);

#[mod_api::__subscriptions]
impl Hooks for Probe {
    fn screen_render_pre(&self, _: &dyn ClientHost, event: &mut ScreenRenderPreEvent<'_>) -> Flow {
        if !matches!(event.screen.id, "title" | "mod") || !self.0.lock().unwrap().replace {
            return Flow::Continue;
        }
        for (rect, label) in [
            (FIRST, "First fixture control"),
            (SECOND, "Second fixture control"),
        ] {
            event
                .draw
                .widget_button(rect, &Text::literal(label), true, false)
                .unwrap();
        }
        Flow::Cancel
    }

    fn screen_action_pre(&self, _: &dyn ClientHost, event: &mut ScreenActionPreEvent<'_>) -> Flow {
        let mut menu = self.0.lock().unwrap();
        if event.screen.id == "world_select"
            && menu.keep_world_list
            && event.input.action == ScreenAction::Back
        {
            return Flow::Cancel;
        }
        if !matches!(event.screen.id, "title" | "mod") || !menu.replace {
            return Flow::Continue;
        }
        match event.input.action {
            ScreenAction::Activate => {
                if let Some(index) = [FIRST, SECOND]
                    .iter()
                    .position(|rect| Some(*rect) == event.input.focused)
                {
                    menu.activated.push((event.input.source, index));
                }
            }
            ScreenAction::Page { forward } => menu.page += if forward { 1 } else { -1 },
            ScreenAction::Scroll { y, .. } => menu.scroll += y,
            ScreenAction::Navigate(_) if menu.freeze_focus => {}
            _ => return Flow::Continue,
        }
        Flow::Cancel
    }
}

struct TestLoop;

impl EventLoopCtl for TestLoop {
    fn exit(&self) {
        panic!("the fixture should not leave the title window");
    }

    fn set_control_flow(&self, _: ControlFlow) {}
}

fn pad(game: &mut Game<'_>, command: &str) {
    let words: Vec<_> = command.split_whitespace().collect();
    game.pad_command(&words).unwrap();
    game.poll_pads(&TestLoop);
    game.poll_pads(&TestLoop);
    assert!(game.error.is_none());
}

fn keyboard(game: &mut Game<'_>, key: KeyCode) {
    game.input_keyboard_used();
    assert!(!game.screen_key_pressed(key, false));
    assert!(game.key_focus_key(&TestLoop, key));
}

fn draw(game: &mut Game<'_>) {
    game.hud(SIZE);
    game.frames += 1;
    horizon_client::widgets::end_frame(game.frames);
}

#[test]
#[ignore = "daily"]
fn controller_and_keyboard_act_on_the_same_mod_widgets_without_moving_hidden_controls() {
    if run_fresh_cases(
        "game::pad_menu::screen_action_tests::controller_and_keyboard_act_on_the_same_mod_widgets_without_moving_hidden_controls",
        "HORIZON_SCREEN_ACTION_FIXTURE",
        &["1"],
    ) {
        return;
    }
    let menu = Arc::new(Mutex::new(Menu {
        replace: true,
        ..Default::default()
    }));
    let mods = vec![ModEntry::new(
        "screen-action-fixture",
        Box::new(Probe(menu.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        with_client_game(|game| {
            let mut builder = EventLoop::builder();
            #[cfg(target_os = "linux")]
            {
                use winit::platform::x11::EventLoopBuilderExtX11;
                builder.with_x11().with_any_thread(true);
            }
            let event_loop = builder.build().unwrap();
            #[allow(deprecated)]
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(PhysicalSize::new(SIZE[0], SIZE[1])),
                    )
                    .unwrap(),
            );
            game.options.offscreen = true;
            game.renderer = Some(game.new_renderer(window.clone()).unwrap());
            game.window = Some(window);
            game.focused = true;
            game.settings.controller.enabled = true;
            game.settings.controller.scroll_speed = 40.0;
            let root = tempfile::tempdir().unwrap();
            game.title = Some(Title::new(root.path().join("saves"), root.path(), ""));
            game.screen = Screen::Playing;
            draw(game);

            keyboard(game, KeyCode::Tab);
            pad(game, "pad down tap");
            pad(game, "pad south tap");
            menu.lock().unwrap().freeze_focus = true;
            pad(game, "pad up tap");
            keyboard(game, KeyCode::Enter);
            assert_eq!(
                menu.lock().unwrap().activated,
                [
                    (ScreenActionSource::Controller, 1),
                    (ScreenActionSource::Keyboard, 1)
                ]
            );
            assert!(matches!(game.title.as_ref().unwrap().mode, Mode::Main));

            pad(game, "pad rb tap");
            game.pad.menu.last_poll = Some(Instant::now() - Duration::from_millis(100));
            pad(game, "pad stick right 0 1");
            pad(game, "pad stick right 0 0");
            let mut state = menu.lock().unwrap();
            assert_eq!(state.page, 1);
            assert_eq!(state.scroll, 2.0);
            state.keep_world_list = true;
            state.replace = false;
            drop(state);

            game.title.as_mut().unwrap().mode = Mode::Select {
                worlds: Vec::new(),
                selected: None,
            };
            draw(game);
            pad(game, "pad east tap");
            assert!(matches!(
                game.title.as_ref().unwrap().mode,
                Mode::Select { .. }
            ));
            menu.lock().unwrap().keep_world_list = false;
            pad(game, "pad east tap");
            assert!(matches!(game.title.as_ref().unwrap().mode, Mode::Main));

            game.title = None;
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            assert!(game.mod_screen_open("screen-action-fixture"));
            let mut state = menu.lock().unwrap();
            state.replace = true;
            state.freeze_focus = false;
            state.activated.clear();
            drop(state);
            draw(game);
            keyboard(game, KeyCode::Tab);
            pad(game, "pad south tap");
            assert_eq!(
                menu.lock().unwrap().activated,
                [(ScreenActionSource::Controller, 0)]
            );
            assert_eq!(game.mod_screen(), Some("screen-action-fixture"));
            pad(game, "pad east tap");
            assert_eq!(game.screen, Screen::Playing);
        });
    });
}
