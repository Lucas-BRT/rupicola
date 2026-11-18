# egui

Shared rules for verification, existing projects and review: [common](../common.md).

Use for immediate-mode tools and visualizations. Crate signals: `egui, eframe`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://github.com/emilk/egui). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Use a matching eframe example and its App/update integration. Keep durable application state outside transient widget construction.

## State and interaction

Use stable IDs for stateful widgets. Schedule expensive work outside frame rendering and request repaint when results arrive. Use virtualized scrolling for large datasets.

## Platform and compatibility

Do not confuse per-frame UI rebuilding with repeated IO. Check accessibility features, text input and selected native/web backend independently.

## Version gotchas

Checked by compiling [the example](../../examples/egui-web-effects/src/main.rs) against eframe 0.37.0; recheck for any other version. `eframe::App` requires `ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`, not `update(&Context, ..)`. `egui::CentralPanel::show` takes `&mut Ui`. `Id::new` is deprecated: use `Id::unique(..)` for a globally unique id. The window rectangle is `ctx.content_rect()`. Popups and ghosts: `egui::Area::new(id).order(Order::Foreground).fixed_pos(pos).show(&ctx, ..)` and `ctx.layer_painter(LayerId::new(Order::Foreground, id))`. Scroll-linked effects use `ScrollArea::show_viewport`, whose viewport rectangle gives the offset.

Found by rendering the example headlessly and looking at the images (not visible from the code): `ctx.animate_bool(id, target)` starts a never-seen id at `target`, so an open animation needs the call to run every frame while the target is still `false`. A `ScrollArea` whose content is only painted (`painter().rect_filled` with `ui.set_height(..)`) reports a content size of zero width and never scrolls: allocate each row with `ui.allocate_exact_size(..)` and paint into the returned rectangle. Painted clickable areas have no accessible name: add `response.widget_info(|| WidgetInfo::labeled(Role::Button, true, "Card 1"))` (`WidgetType` is deprecated in favor of `egui::Role`). Default controls are about 20 px high, below the 24 px target size.

Headless capture works with `egui_kittest` (features `wgpu` and `snapshot`) on software Vulkan: run `env -u DISPLAY -u WAYLAND_DISPLAY VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json cargo test` (the lavapipe ICD file path varies by distribution). `Harness::builder().wgpu().with_size(..).with_step_dt(1.0 / 60.0).build_ui_state(|ui, app| app.show(ui), app)`; `harness.render()` returns an RGBA image; `get_by_label(..).click()`, `key_press`, `hover_at` and `event` drive input; `run()` stops after 4 steps, so use `run_steps(n)` for animations. A rendering comparison can pass only because of the pointer cursor: keep the pointer in the same place before and after.
