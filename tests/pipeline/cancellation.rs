use super::*;

#[test]
fn scan_cancellation_precedes_installation_access_and_source_parsing() {
    let fixture = fixture();
    let cancelled = Control::default();
    cancelled.cancel();
    let error = compiler::plan(&fixture.0.join("missing"), &cancelled, &mut |_| {}).unwrap_err();
    assert_eq!(error.to_string(), "Operation cancelled");

    let game = fixture.0.join("game");
    write(&game, "interface/example.gfx", "broken = {");
    let control = Control::default();
    let mut updates = 0;
    let error = compiler::plan(&game, &control, &mut |_| {
        updates += 1;
        control.cancel();
    })
    .unwrap_err();
    assert_eq!(updates, 1);
    assert_eq!(error.to_string(), "Operation cancelled");
}

#[test]
fn cancellation_during_work_preserves_previous_mod() {
    let fixture = fixture();
    let control = Control::default();
    let profile = Profile::default();
    let output = fixture.0.join("mod");
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    let before = fs::read(output.join(compiler::manifest_name())).unwrap();
    let cancelled = Control::default();
    let result = compiler::build(&plan, &profile, &output, &cancelled, &mut |p| {
        if p.stage == compiler::ProgressStage::Textures {
            cancelled.cancel();
        }
    });
    assert!(result.is_err());
    assert_eq!(
        fs::read(output.join(compiler::manifest_name())).unwrap(),
        before
    );
}
