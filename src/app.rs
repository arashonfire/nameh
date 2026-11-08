use crate::{files, window::DocumentWindow};
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const APPLICATION_ID: &str = "io.github.arash.mdview";
type Windows = Rc<RefCell<Vec<Rc<DocumentWindow>>>>;

pub fn run() -> glib::ExitCode {
    glib::set_application_name("mdview");
    let application = gtk::Application::builder()
        .application_id(APPLICATION_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    application.add_main_option(
        "prototype",
        glib::Char::from(b'p'),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        "Show the bundled native rendering prototype",
        None,
    );
    let preview = Rc::new(Cell::new(false));
    let option = preview.clone();
    application.connect_handle_local_options(move |_, options| {
        option.set(options.contains("prototype"));
        std::ops::ControlFlow::Continue(())
    });
    application.connect_startup(|_| gtk::Window::set_default_icon_name(APPLICATION_ID));
    configure(&application, preview);

    application.run()
}

fn configure(application: &gtk::Application, preview: Rc<Cell<bool>>) -> Windows {
    let settings = crate::reading::ReadingSettings::new(if cfg!(test) {
        None
    } else {
        Some(mdview::preferences::Preferences::path())
    });
    settings.install(application);
    let windows: Windows = Rc::new(RefCell::new(Vec::new()));
    let registry_for_return = windows.clone();
    let registry = windows.clone();
    let reading = settings.clone();
    application.connect_activate(move |application| {
        if let Some(window) = application.active_window() {
            window.present();
        } else {
            new_window(application, &registry, preview.get(), &reading);
        }
    });
    let registry = windows.clone();
    let reading = settings.clone();
    application.connect_open(move |application, files, _| {
        open_files(application, &registry, files, &reading);
    });
    let new = gio::SimpleAction::new("new-window", None);
    let weak = application.downgrade();
    new.connect_activate(move |_, _| {
        if let Some(application) = weak.upgrade() {
            new_window(&application, &windows, false, &settings);
        }
    });
    application.add_action(&new);
    let quit = gio::SimpleAction::new("quit", None);
    let weak = application.downgrade();
    quit.connect_activate(move |_, _| {
        if let Some(application) = weak.upgrade() {
            application.quit();
        }
    });
    application.add_action(&quit);
    application.set_accels_for_action("app.quit", &["<Control>q"]);
    application.set_accels_for_action("app.new-window", &["<Control>n"]);
    application.set_accels_for_action("win.open", &["<Control>o"]);
    application.set_accels_for_action("win.close", &["<Control>w"]);
    for (action, keys) in [
        ("win.find", vec!["<Control>f"]),
        ("win.find-next", vec!["<Control>g"]),
        ("win.find-previous", vec!["<Control><Shift>g"]),
        (
            "app.zoom-in",
            vec!["<Control>plus", "<Control>equal", "<Control>KP_Add"],
        ),
        (
            "app.zoom-out",
            vec!["<Control>minus", "<Control>KP_Subtract"],
        ),
        ("app.zoom-reset", vec!["<Control>0", "<Control>KP_0"]),
    ] {
        application.set_accels_for_action(action, &keys);
    }
    registry_for_return
}

fn new_window(
    application: &gtk::Application,
    registry: &Windows,
    preview: bool,
    settings: &Rc<crate::reading::ReadingSettings>,
) -> Rc<DocumentWindow> {
    let controller = DocumentWindow::new(
        application,
        preview,
        Rc::new(files::launch),
        settings.clone(),
    );
    let weak = Rc::downgrade(registry);
    let window = controller.window.downgrade();
    controller.window.connect_close_request(move |_| {
        if let (Some(registry), Some(window)) = (weak.upgrade(), window.upgrade()) {
            registry
                .borrow_mut()
                .retain(|controller| controller.window != window);
        }
        glib::Propagation::Proceed
    });
    registry.borrow_mut().push(controller.clone());
    controller
}

fn open_files(
    application: &gtk::Application,
    registry: &Windows,
    files: &[gio::File],
    settings: &Rc<crate::reading::ReadingSettings>,
) {
    for (index, file) in files.iter().enumerate() {
        let active = application.active_window();
        let existing = (index == 0)
            .then(|| {
                registry
                    .borrow()
                    .iter()
                    .find(|controller| {
                        !controller.preview
                            && Some(controller.window.upcast_ref::<gtk::Window>())
                                == active.as_ref()
                    })
                    .cloned()
            })
            .flatten();
        let controller =
            existing.unwrap_or_else(|| new_window(application, registry, false, settings));
        controller.open_file(file.clone());
        controller.window.present();
    }
}

#[cfg(test)]
pub fn verify_open_signals() {
    let application = gtk::Application::builder()
        .application_id("io.github.arash.mdview.TestOpenSignals")
        .flags(gio::ApplicationFlags::NON_UNIQUE | gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    let registry = configure(&application, Rc::new(Cell::new(false)));
    application.register(gio::Cancellable::NONE).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let basic = gio::File::for_path(root.join("basic.md"));
    application.open(std::slice::from_ref(&basic), "");
    let first = registry.borrow()[0].clone();
    crate::window::wait_for_open(&first);
    assert_eq!(registry.borrow().len(), 1);
    let unicode = gio::File::for_path(root.join("notes/Unicode café.md"));
    application.open(&[unicode, basic], "");
    let controllers = registry.borrow().clone();
    for controller in &controllers {
        crate::window::wait_for_open(controller);
    }
    assert_eq!(
        registry.borrow().len(),
        2,
        "one document per window for multi-file opening"
    );
    assert_eq!(
        first.state.borrow().current_path,
        Some(root.join("notes/Unicode café.md"))
    );
    crate::window::verify_reading(&first, &application);
    crate::window::verify_reload(&application);
    application.activate_action("new-window", None);
    assert_eq!(registry.borrow().len(), 3);
    assert!(registry.borrow()[2].state.borrow().current_path.is_none());
    let controllers = registry.borrow().clone();
    for controller in controllers {
        controller.window.close();
    }
    assert!(
        registry.borrow().is_empty(),
        "closed windows must release their controller"
    );
}
