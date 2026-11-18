use super::*;
use crate::{
    assets::{Installation, Source},
    compiler::TextureAsset,
    config::Category,
    fonts,
    test_support::Temp,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

fn workspace(count: usize) -> (Temp, Studio, Arc<egui::ColorImage>) {
    let temp = Temp::new();
    let path = temp.0.join("icon.png");
    let image = RgbaImage::from_pixel(16, 16, image::Rgba([50, 150, 200, 255]));
    image.save(&path).unwrap();
    let mut studio = Studio::new(None);
    studio.plan = Some(Arc::new(BuildPlan {
        game: Installation {
            root: temp.0.clone(),
            source_mods: vec![],
            version: String::new(),
            resources: (0..count)
                .map(|i| (format!("gfx/{i}.png"), Source::File(path.clone())))
                .collect(),
            archives: 0,
        },
        textures: (0..count)
            .map(|i| TextureAsset {
                path: format!("gfx/{i}.png"),
                width: 16,
                height: 16,
                frames: 1,
                category: Category::Icon,
                tiled: false,
            })
            .collect(),
        fonts: fonts::Catalog::default(),
        sources: BTreeMap::new(),
        sprites: vec![],
        diagnostics: vec![],
    }));
    studio.visual_search.open = true;
    let color = Arc::new(egui::ColorImage::from_rgba_unmultiplied(
        [16, 16],
        image.as_raw(),
    ));
    (temp, studio, color)
}

fn finish(search: &mut VisualSearch, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while search.job.is_some() {
        assert!(Instant::now() < deadline, "Search worker timed out");
        search.poll(ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn pasted_image_search_finishes_in_background_and_reuses_index() {
    let (_temp, mut studio, color) = workspace(1);
    let ctx = egui::Context::default();
    let run_query = |studio: &mut Studio| {
        let input = egui::RawInput {
            events: vec![egui::Event::PasteImage(Arc::clone(&color))],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            studio.visual_search_window(ui.ctx(), false);
        })
        .drop_without_applying_deltas();
        assert!(studio.visual_search.job.is_some());
        finish(&mut studio.visual_search, &ctx);
        assert_eq!(
            studio.visual_search.rows.len(),
            1,
            "{}",
            studio.visual_search.status
        );
        assert!(studio.visual_search.query.is_some());
    };
    run_query(&mut studio);
    let index = studio.visual_search.index.clone().unwrap();
    run_query(&mut studio);
    assert!(Arc::ptr_eq(
        &index,
        studio.visual_search.index.as_ref().unwrap()
    ));
    studio.visual_search.start(
        &ctx,
        Arc::clone(studio.plan.as_ref().unwrap()),
        studio.profile.clone(),
        Input::Image(color),
    );
    let control = studio.visual_search.job.as_ref().unwrap().control.clone();
    studio.visual_search = VisualSearch::default();
    assert!(control.check().is_err());
}

#[test]
fn paste_is_consumed_once_across_layout_passes() {
    let ctx = egui::Context::default();
    let mut search = VisualSearch::default();
    let image = Arc::new(egui::ColorImage::filled([2, 2], egui::Color32::WHITE));
    let input = egui::RawInput {
        events: vec![egui::Event::PasteImage(image)],
        ..Default::default()
    };
    ctx.run_ui(input, |ui| {
        let first = ui.push_id("first-pass", |ui| search.controls(ui)).inner;
        let second = ui.push_id("second-pass", |ui| search.controls(ui)).inner;
        assert!(matches!(first, Some(Input::Image(_))));
        assert!(second.is_none());
    })
    .drop_without_applying_deltas();
}

#[test]
fn pages_extend_past_sixty_without_duplicates_and_can_resume_after_cancellation() {
    let (_temp, mut studio, color) = workspace(67);
    let ctx = egui::Context::default();
    let plan = Arc::clone(studio.plan.as_ref().unwrap());
    let search = &mut studio.visual_search;
    search.start(
        &ctx,
        Arc::clone(&plan),
        studio.profile.clone(),
        Input::Image(Arc::clone(&color)),
    );
    finish(search, &ctx);
    assert_eq!((search.rows.len(), search.ranked.len()), (20, 67));
    let index = search.index.clone().unwrap();

    search.load_next_page(&ctx, Arc::clone(&plan));
    let control = search.job.as_ref().unwrap().control.clone();
    search.load_next_page(&ctx, Arc::clone(&plan));
    assert!(
        control.check().is_ok(),
        "An in-flight page must not be replaced"
    );
    search.cancel();
    assert!(control.check().is_err());
    assert_eq!(search.rows.len(), 20);
    assert!(search.paging_paused);

    for expected in [40, 60, 67] {
        search.load_next_page(&ctx, Arc::clone(&plan));
        finish(search, &ctx);
        assert_eq!(search.rows.len(), expected, "{}", search.status);
    }
    assert_eq!(
        search
            .rows
            .iter()
            .map(|row| row.hit.asset)
            .collect::<Vec<_>>(),
        (0..67).collect::<Vec<_>>()
    );
    assert!(Arc::ptr_eq(&index, search.index.as_ref().unwrap()));
    search.load_next_page(&ctx, Arc::clone(&plan));
    assert!(search.job.is_none());

    search.start(&ctx, plan, studio.profile.clone(), Input::Image(color));
    assert!(search.rows.is_empty());
    assert!(search.ranked.is_empty());
    finish(search, &ctx);
    assert_eq!(search.rows.len(), PAGE_SIZE);
    assert_eq!(search.rows[0].hit.asset, 0);

    search.load_next_page(&ctx, Arc::clone(studio.plan.as_ref().unwrap()));
    let old_control = search.job.as_ref().unwrap().control.clone();
    search.start(
        &ctx,
        Arc::clone(studio.plan.as_ref().unwrap()),
        studio.profile.clone(),
        Input::Image(Arc::new(egui::ColorImage::filled(
            [16, 16],
            egui::Color32::WHITE,
        ))),
    );
    assert!(old_control.check().is_err());
    finish(search, &ctx);
    assert_eq!(search.rows.len(), PAGE_SIZE);
    assert_eq!(search.rows[0].hit.asset, 0);
}

#[test]
fn scrolling_to_the_bottom_requests_the_next_page() {
    let (_temp, mut studio, color) = workspace(45);
    let ctx = egui::Context::default();
    let plan = Arc::clone(studio.plan.as_ref().unwrap());
    let search = &mut studio.visual_search;
    search.start(
        &ctx,
        Arc::clone(&plan),
        studio.profile.clone(),
        Input::Image(color),
    );
    finish(search, &ctx);
    for frame in 0..10 {
        let mut events = vec![egui::Event::PointerMoved(egui::pos2(100.0, 100.0))];
        if frame > 0 {
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                delta: egui::vec2(0.0, -2400.0),
                modifiers: egui::Modifiers::NONE,
            });
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            time: Some(f64::from(frame) * 0.1),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            let response = search.results(ui, &plan, None, false);
            if response.load_more {
                search.load_next_page(&ctx, Arc::clone(&plan));
            }
        })
        .drop_without_applying_deltas();
        if frame == 0 {
            assert!(
                search.job.is_none(),
                "The first page should not load more at the top"
            );
            search.paging_paused = true;
        }
        if search.job.is_some() {
            break;
        }
    }
    assert!(
        search.job.is_some(),
        "Scrolling down should start a page worker"
    );
    finish(search, &ctx);
    assert_eq!(search.rows.len(), 40);
}

#[test]
fn closing_a_page_worker_preserves_results_and_allows_automatic_loading_after_reopening() {
    let (_temp, mut studio, color) = workspace(45);
    let ctx = egui::Context::default();
    let plan = Arc::clone(studio.plan.as_ref().unwrap());
    let search = &mut studio.visual_search;
    search.start(
        &ctx,
        Arc::clone(&plan),
        studio.profile.clone(),
        Input::Image(color),
    );
    finish(search, &ctx);
    search.load_next_page(&ctx, Arc::clone(&plan));
    let control = search.job.as_ref().unwrap().control.clone();
    search.close();
    assert!(control.check().is_err());
    assert!(!search.open);
    assert!(search.job.is_none());
    assert!(!search.paging_paused);
    assert_eq!(search.rows.len(), PAGE_SIZE);

    search.open = true;
    search.load_next_page(&ctx, plan);
    finish(search, &ctx);
    assert_eq!(search.rows.len(), PAGE_SIZE * 2);
}

#[test]
fn failed_page_keeps_its_position_and_can_be_retried() {
    let (temp, mut studio, color) = workspace(45);
    let ctx = egui::Context::default();
    let plan = Arc::clone(studio.plan.as_ref().unwrap());
    let search = &mut studio.visual_search;
    search.start(
        &ctx,
        Arc::clone(&plan),
        studio.profile.clone(),
        Input::Image(color),
    );
    finish(search, &ctx);
    let path = temp.0.join("icon.png");
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    search.load_next_page(&ctx, Arc::clone(&plan));
    finish(search, &ctx);
    assert!(search.paging_paused);
    assert!(search.status.contains("Cannot read"), "{}", search.status);
    assert_eq!((search.rows.len(), search.ranked.len()), (PAGE_SIZE, 45));

    std::fs::write(path, bytes).unwrap();
    search.load_next_page(&ctx, plan);
    finish(search, &ctx);
    assert!(!search.paging_paused);
    assert_eq!(search.rows.len(), PAGE_SIZE * 2);
    assert_eq!(search.rows[PAGE_SIZE].hit.asset, PAGE_SIZE);
}
