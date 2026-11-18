use super::*;
use std::sync::{Arc, Mutex};
use winit::dpi::PhysicalPosition;

#[derive(Debug, PartialEq)]
struct Frame {
    pointer: [i32; 2],
    size: [i32; 2],
    origin: [f32; 2],
}

#[derive(Default)]
struct Seen {
    draw: Vec<Frame>,
    tooltip_at: Vec<[f32; 2]>,
}

const POS: Pos = (17, 63, -9);

struct Position(Arc<Mutex<Seen>>);
#[mod_api::sunrise_part]
impl mod_api::BlockPart for Position {}
#[mod_api::sunrise_part]
impl mod_api::BlockEntityPart for Position {
    fn screen_draw(&self, _: &dyn mod_api::ClientHost, event: &mut mod_api::ScreenDrawEvent<'_>) {
        assert_eq!(event.pos, Some([17, 63, -9]));
        self.0.lock().unwrap().draw.push(Frame {
            pointer: event.pointer,
            size: event.gui_size,
            origin: event.origin,
        });
    }

    fn screen_tooltip(
        &self,
        _: &dyn mod_api::ClientHost,
        event: &mut mod_api::ScreenTooltipEvent<'_>,
    ) {
        let mut seen = self.0.lock().unwrap();
        let draw = seen.draw.last().unwrap();
        assert_eq!(event.pointer, draw.pointer);
        assert_eq!(event.gui_size, draw.size);
        assert_eq!(event.origin, draw.origin);
        seen.tooltip_at.push(event.at);
    }
}
struct Listener(Arc<Mutex<Seen>>);
#[mod_api::__subscriptions]
impl mod_api::Hooks for Listener {
    fn register(&self, r: &mut dyn mod_api::Registrar, _: &mod_api::RegistrationContext) {
        let kind = r
            .block_entity_type("screen")
            .blocks(&["minecraft:chest"])
            .build();
        r.block_entity_part(&kind, Position(self.0.clone()))
            .unwrap();
    }
}
#[test]
#[ignore = "daily"]
fn screen_draw_receives_open_blocks_position() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "test",
        Box::new(Listener(seen.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods.clone(), || {
        mod_platform::__private::part_tests::install_parts_fixture(&mods).unwrap();
        crate::game::native_entity_refs::tests::with_client_game(|game| {
            game.title = None;
            game.screen = Screen::Inventory;
            game.ui.hide_gui = true;
            game.settings.gui_scale = 2;
            game.level.recipe_book.open = [false; 4];
            game.container = Some(POS);
            game.world
                .block_entities
                .insert(POS, BlockEntity::Chest(vec![None; 27]));
            game.mod_screen = Some(OpenNativeScreen {
                layout_revision: 0,
                slot_moves: Default::default(),
                key: POS,
                window: 1,
                block: "minecraft:chest".into(),
                screen: mod_api::Screen {
                    width: 176.0,
                    height: 166.0,
                    inventory_x: 8.0,
                    inventory_y: 84.0,
                    own_labels: true,
                    own_background: true,
                    ..Default::default()
                },
                data: Vec::new(),
                extra: None,
            });

            // No new motion: the resize allows the requested GUI scale of 2.
            game.set_window_cursor(PhysicalPosition::new(321.75, 231.75), 1.0);
            game.hud_layers([638, 478]);
            game.hud_layers([640, 480]);

            // A cursor supplied in GUI pixels directly still truncates toward zero.
            game.window_cursor = None;
            game.cursor = (-3.75, 129.75);
            game.hud_layers([640, 480]);
            let seen = seen.lock().unwrap();
            assert_eq!(
                seen.draw,
                [
                    Frame {
                        pointer: [321, 231],
                        size: [638, 478],
                        origin: [231.0, 156.0],
                    },
                    Frame {
                        pointer: [160, 115],
                        size: [320, 240],
                        origin: [72.0, 37.0],
                    },
                    Frame {
                        pointer: [-3, 129],
                        size: [320, 240],
                        origin: [72.0, 37.0],
                    },
                ]
            );
            assert_eq!(
                seen.tooltip_at,
                [[90.75, 75.75], [88.875, 78.875], [-75.75, 92.75]]
            );
        });
    });
}
