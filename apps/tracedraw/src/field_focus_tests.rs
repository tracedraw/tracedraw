//! Ctrl+K and the other command shortcuts after a property bar field
//! was typed into: the video of #2893 typed in the bar, then pressed
//! Ctrl+K, and nothing broke apart.
use super::*;

/// The rectangle of `setup` with a drop shadow, the bar's width field
/// clicked and "60" typed in it, not yet entered.
fn typed_in_the_width_field() -> (egui::Context, App, ShapeId) {
    let (ctx, mut app, id) = setup();
    app.set_shadow(Some(tracedraw_core::Shadow::default()), false);
    let texts = texts_drawn(&ctx, &mut app, Vec::new());
    let at = text_at(&texts, "50.0 mm").expect("the width field");
    click_at(&ctx, &mut app, at, Modifiers::NONE);
    assert!(
        ctx.egui_wants_keyboard_input(),
        "the field has the keyboard"
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::A, Modifiers::COMMAND)],
        Modifiers::COMMAND,
    );
    frame(
        &ctx,
        &mut app,
        vec![key_up(egui::Key::A, Modifiers::COMMAND)],
        Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![Event::Text("60".into())],
        Modifiers::NONE,
    );
    assert!(ctx.egui_wants_keyboard_input(), "still typing");
    (ctx, app, id)
}

fn tap(ctx: &egui::Context, app: &mut App, k: egui::Key, m: Modifiers) {
    frame(ctx, app, vec![key(k, m)], m);
    frame(ctx, app, vec![key_up(k, m)], Modifiers::NONE);
    frame(ctx, app, Vec::new(), Modifiers::NONE);
}

fn broken_apart(app: &App, id: ShapeId) -> bool {
    app.doc().find_shape(id).expect("rect").shadow.is_none() && shape_count(app) == 2
}

/// Enter takes the value and gives the keyboard back to the drawing:
/// Ctrl+K then breaks the shadow off.
#[test]
fn ctrl_k_after_enter_in_a_bar_field() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    tap(&ctx, &mut app, egui::Key::Enter, Modifiers::NONE);
    assert!(
        close(bounds(&app, id).width(), 60.0),
        "{:?}",
        bounds(&app, id)
    );
    assert!(!ctx.egui_wants_keyboard_input(), "back to the drawing");
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    assert!(broken_apart(&app, id));
}

/// A click on the object takes the value and gives the keyboard back
/// to the drawing, the object still selected.
#[test]
fn ctrl_k_after_a_click_on_the_object() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    let on = app.view.to_screen(Point::new(30.0, 25.0));
    click_at(&ctx, &mut app, on, Modifiers::NONE);
    assert!(
        close(bounds(&app, id).width(), 60.0),
        "{:?}",
        bounds(&app, id)
    );
    assert!(!ctx.egui_wants_keyboard_input(), "back to the drawing");
    assert_eq!(app.selection, vec![id]);
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    assert!(broken_apart(&app, id));
}

/// Ctrl+K pressed while the field still has the keyboard: the field
/// takes its value and the command runs all the same.
#[test]
fn ctrl_k_while_a_bar_field_has_the_keyboard() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    for _ in 0..2 {
        frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    }
    assert!(broken_apart(&app, id), "{}", shape_count(&app));
    assert!(
        close(bounds(&app, id).width(), 60.0),
        "{:?}",
        bounds(&app, id)
    );
    assert!(!ctx.egui_wants_keyboard_input(), "back to the drawing");
}

/// The keys that edit the text of a field stay the field's: Ctrl+Z,
/// Ctrl+C and Ctrl+A do not undo, copy or select in the drawing.
#[test]
fn editing_keys_stay_in_a_bar_field() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    let steps = app.engine.history_labels().0.len();
    let selected = app.selection.clone();
    for k in [
        egui::Key::Z,
        egui::Key::C,
        egui::Key::A,
        egui::Key::X,
        egui::Key::V,
    ] {
        frame(
            &ctx,
            &mut app,
            vec![key(k, Modifiers::COMMAND)],
            Modifiers::COMMAND,
        );
        frame(
            &ctx,
            &mut app,
            vec![key_up(k, Modifiers::COMMAND)],
            Modifiers::NONE,
        );
        assert!(ctx.egui_wants_keyboard_input(), "{k:?}: the field keeps it");
    }
    assert_eq!(app.engine.history_labels().0.len(), steps, "nothing undone");
    assert_eq!(app.selection, selected);
    assert_eq!(shape_count(&app), 1, "nothing pasted or cut");
    assert!(app.doc().find_shape(id).expect("rect").shadow.is_some());
}

/// Ctrl+S in the code of the macro editor (Alt+F11) saves the macro, as
/// its Save button says, and the code keeps the keyboard. Since #2893
/// the code gave the keyboard up and Ctrl+S went to the drawing: the
/// Save Drawing dialog opened and the macro stayed unsaved.
#[test]
fn ctrl_s_in_the_macro_code_saves_the_macro() {
    let (ctx, mut app, _) = setup();
    let dir = std::env::temp_dir().join(format!("tracedraw-ctrl-s-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    app.macros.dir = Some(dir.clone());
    app.open_macro_editor();
    assert!(app.macros.editor.is_some(), "{}", app.status);
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    let code = egui::Id::new("macro_code_edit");
    ctx.memory_mut(|m| m.request_focus(code));
    frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
    frame(
        &ctx,
        &mut app,
        vec![Event::Text("'".into())],
        Modifiers::NONE,
    );
    assert!(app.macros.editor.as_ref().expect("editor").dirty, "typed");
    tap(&ctx, &mut app, egui::Key::S, Modifiers::COMMAND);
    assert!(
        matches!(app.dialog, crate::ui::dialogs::Dialog::None),
        "{:?}",
        app.dialog
    );
    assert!(!app.macros.editor.as_ref().expect("editor").dirty, "saved");
    assert_eq!(ctx.memory(|m| m.focused()), Some(code));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Ctrl itself arrives as a key press (on Windows with Ctrl already
/// held): the field keeps the keyboard, and the Ctrl+V after it pastes
/// into the field, not on the drawing (#3198).
#[test]
fn ctrl_alone_keeps_the_keyboard_in_a_bar_field() {
    let (ctx, mut app, id) = typed_in_the_width_field();
    for k in [egui::Key::ControlLeft, egui::Key::ControlRight] {
        frame(
            &ctx,
            &mut app,
            vec![key(k, Modifiers::COMMAND)],
            Modifiers::COMMAND,
        );
        frame(&ctx, &mut app, Vec::new(), Modifiers::COMMAND);
        assert!(ctx.egui_wants_keyboard_input(), "{k:?}: the field keeps it");
    }
    frame(
        &ctx,
        &mut app,
        vec![Event::Paste("5".into())],
        Modifiers::COMMAND,
    );
    frame(
        &ctx,
        &mut app,
        vec![key_up(egui::Key::ControlLeft, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert!(ctx.egui_wants_keyboard_input(), "still typing");
    assert_eq!(shape_count(&app), 1, "nothing pasted on the drawing");
    tap(&ctx, &mut app, egui::Key::Enter, Modifiers::NONE);
    assert!(
        close(bounds(&app, id).width(), 605.0),
        "the paste went into the field: {:?}",
        bounds(&app, id)
    );
}

/// A text with "Hello" typed and left, then edited again with " world"
/// typed at its end: two undo steps.
fn editing_hello_world() -> (egui::Context, App, ShapeId) {
    let (ctx, mut app, _) = setup();
    app.set_tool(Tool::Text);
    app.start_text(Point::new(20.0, 80.0), None);
    app.text_insert("Hello");
    let id = app.text_edit.as_ref().expect("editing").shape;
    app.finish_text();
    app.begin_text_edit(id);
    app.text_insert(" world");
    assert_eq!(app.edit_text(), "Hello world");
    (ctx, app, id)
}

/// Ctrl+Z while a text is being typed undoes the typing and the text is
/// edited on; Ctrl+Shift+Z brings it back (#3108: Ctrl+Z did nothing
/// until the editing was left).
#[test]
fn ctrl_z_while_typing_undoes_and_the_text_is_edited_on() {
    let (ctx, mut app, id) = editing_hello_world();
    tap(&ctx, &mut app, egui::Key::Z, Modifiers::COMMAND);
    assert_eq!(
        app.text_edit.as_ref().map(|t| t.shape),
        Some(id),
        "still editing"
    );
    assert_eq!(app.edit_text(), "Hello");
    tap(
        &ctx,
        &mut app,
        egui::Key::Z,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert_eq!(app.text_edit.as_ref().map(|t| t.shape), Some(id));
    assert_eq!(app.edit_text(), "Hello world");
    // Typing goes on at the caret, into a step of its own.
    frame(
        &ctx,
        &mut app,
        vec![Event::Text("!".into())],
        Modifiers::NONE,
    );
    assert_eq!(app.edit_text(), "Hello world!");
    tap(&ctx, &mut app, egui::Key::Z, Modifiers::COMMAND);
    assert_eq!(app.edit_text(), "Hello world");
    // Alt+Backspace undoes too; undoing the text's creation ends the
    // editing.
    tap(&ctx, &mut app, egui::Key::Backspace, Modifiers::ALT);
    assert_eq!(app.edit_text(), "Hello");
    tap(&ctx, &mut app, egui::Key::Z, Modifiers::COMMAND);
    assert!(app.text_edit.is_none());
    assert!(app.doc().find_shape(id).is_none());
}

/// Ctrl+Shift+T while a text is being typed with the Text tool opens
/// Edit Text for it (#3266).
#[test]
fn ctrl_shift_t_while_typing_opens_edit_text() {
    let (ctx, mut app, id) = editing_hello_world();
    tap(
        &ctx,
        &mut app,
        egui::Key::T,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    match &app.dialog {
        crate::ui::dialogs::Dialog::EditText { shape, text, .. } => {
            assert_eq!(*shape, id);
            assert_eq!(text, "Hello world");
        }
        d => panic!("{d:?}"),
    }
    assert_eq!(app.doc().find_shape(id).map(|_| ()), Some(()));
}

/// A tool or a command given F13 to F24 (Options > Customization >
/// Commands) is picked or runs while a text is being typed; the
/// typing's own Ctrl keys stay its own (#3251).
#[test]
fn keys_f13_to_f24_given_a_command_work_while_typing() {
    let (ctx, mut app, id) = editing_hello_world();
    app.settings
        .shortcuts
        .push((Tool::Rectangle.id().to_string(), "F13".into()));
    app.settings
        .shortcuts
        .push(("menu.view.rulers".to_string(), "Shift+F21".into()));
    // Ctrl+B is the typing's bold, Ctrl+K (no use in typing) nothing.
    tap(&ctx, &mut app, egui::Key::K, Modifiers::COMMAND);
    assert_eq!(app.edit_text(), "Hello world");
    assert!(app.text_edit.is_some());
    tap(&ctx, &mut app, egui::Key::F13, Modifiers::NONE);
    assert_eq!(app.tool, Tool::Rectangle);
    assert!(app.text_edit.is_none(), "the editing ended for the tool");
    assert!(app.doc().find_shape(id).is_some());
    app.set_tool(Tool::Text);
    app.begin_text_edit(id);
    let rulers = app.show_rulers;
    tap(&ctx, &mut app, egui::Key::F21, Modifiers::SHIFT);
    assert_eq!(app.show_rulers, !rulers, "the command ran");
    assert_eq!(app.edit_text(), "Hello world");
}
