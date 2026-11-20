//! AppKit objects live exclusively on the main thread. Rebuild labels when
//! the menu opens; the host posts an icon change only on claim / release.
use super::{agent, local_state, model};
use anyhow::{Context, Result};
use objc2::{define_class, msg_send, rc::Retained, runtime::ProtocolObject, sel, MainThreadOnly};
use objc2_app_kit::*;
use objc2_foundation::{ns_string, MainThreadMarker, NSObject, NSObjectProtocol, NSString};
use std::cell::OnceCell;

thread_local! { static ITEM: OnceCell<Retained<NSStatusItem>> = const { OnceCell::new() }; }

fn icon() {
    ITEM.with(|cell| {
        let Some(item) = cell.get() else { return };
        let state = local_state();
        let symbol = if state.viewer.is_some() {
            "rectangle.fill.on.rectangle.fill"
        } else {
            "rectangle.on.rectangle"
        };
        if let Some(button) = item.button(MainThreadMarker::new().unwrap()) {
            if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str(symbol),
                Some(ns_string!("Sunna host")),
            ) {
                image.setTemplate(true);
                button.setImage(Some(&image));
            } else {
                button.setTitle(ns_string!("Sunna"));
            }
        }
    });
}
pub(super) fn refresh() {
    dispatch2::DispatchQueue::main().exec_async(icon);
}

fn confirm(message: &str, action: &str) -> bool {
    let dialog = NSAlert::new(MainThreadMarker::new().unwrap());
    dialog.setMessageText(&NSString::from_str(message));
    dialog.addButtonWithTitle(ns_string!("Cancel"));
    dialog.addButtonWithTitle(&NSString::from_str(action));
    dialog.runModal() == NSAlertSecondButtonReturn
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "SunnaHostMenuActions"]
    struct Actions;
    unsafe impl NSObjectProtocol for Actions {}
    unsafe impl NSMenuDelegate for Actions {
        #[unsafe(method(menuNeedsUpdate:))]
        fn update(&self, menu: &NSMenu) {
            let s = local_state();
            let rows = model::menu(s.capture_reason.as_deref(), &s.reach, s.address, s.viewer.as_deref());
            for (i,text) in rows.iter().enumerate() {
                if let Some(item) = menu.itemAtIndex(i as isize) { item.setTitle(&NSString::from_str(text)); }
            }
        }
    }
    impl Actions {
        #[unsafe(method(openSunna:))]
        fn open(&self, _: &NSMenuItem) {
            if let Err(e) = open_window() { tracing::error!(%e, "opening Sunna"); }
        }
        #[unsafe(method(copyLink:))]
        fn copy_link(&self, _: &NSMenuItem) {
            let pasteboard = NSPasteboard::generalPasteboard();
            pasteboard.clearContents();
            pasteboard.setString_forType(&NSString::from_str(&local_state().link), unsafe { NSPasteboardTypeString });
        }
        #[unsafe(method(stopSharing:))]
        fn stop(&self, _: &NSMenuItem) {
            end_sharing(false);
        }
        #[unsafe(method(quitHost:))]
        fn quit(&self, _: &NSMenuItem) {
            end_sharing(true);
        }
    }
);

fn end_sharing(quit: bool) {
    let result = (|| -> Result<()> {
        let u = sunna_update::Updater::new("app")?;
        let _operation = u.lock("mac-host-management.lock", false)?;
        // Hold admission closed through the idle case. An existing slot
        // takes the confirmation path even if its name is still arriving.
        let idle = u.lock("session.lock", false);
        let state = local_state();
        let viewer = state
            .viewer
            .as_deref()
            .or_else(|| idle.is_err().then_some("Someone"));
        let action = if quit {
            model::Action::Quit
        } else {
            model::Action::Stop
        };
        if let Some(message) = model::confirmation(quit, state.agent, viewer) {
            if !confirm(&message, action.label()) {
                return Ok(());
            }
        }
        if quit && state.agent {
            agent::set_paused(true)?;
        }
        if !quit && agent::enabled() {
            agent::remove()?;
        }
        std::process::exit(0);
    })();
    if let Err(error) = result {
        let alert = NSAlert::new(MainThreadMarker::new().unwrap());
        alert.setMessageText(&NSString::from_str(&format!(
            "Could not stop sharing: {error:#}"
        )));
        alert.runModal();
    }
}

fn open_window() -> Result<()> {
    for app in
        NSRunningApplication::runningApplicationsWithBundleIdentifier(ns_string!("dev.sunna.app"))
            .iter()
    {
        if app.activationPolicy() == NSApplicationActivationPolicy::Regular {
            #[allow(deprecated)]
            if app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows) {
                return Ok(());
            }
        }
    }
    let exe = agent::executable()?;
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .context("Open the installed Sunna.app")?;
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-n")
        .arg(bundle)
        .status()?;
    anyhow::ensure!(status.success(), "Cannot open Sunna.app");
    Ok(())
}

pub fn run(ready: impl FnOnce() + Send + 'static) -> Result<()> {
    let mtm = MainThreadMarker::new().context("Menu must run on main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    anyhow::ensure!(
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory),
        "Cannot enter accessory mode"
    );
    let actions: Retained<Actions> = unsafe { msg_send![Actions::alloc(mtm), init] };
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSSquareStatusItemLength);
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    for _ in 0..3 {
        let row = NSMenuItem::new(mtm);
        row.setEnabled(false);
        menu.addItem(&row);
    }
    for row in model::ACTIONS {
        let Some(action) = row else {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            continue;
        };
        let label = action.label();
        let action = match action {
            model::Action::Open => sel!(openSunna:),
            model::Action::Copy => sel!(copyLink:),
            model::Action::Stop => sel!(stopSharing:),
            model::Action::Quit => sel!(quitHost:),
        };
        // Each selector above takes one menu item and its retained target
        // outlives the application's run loop.
        unsafe {
            let row = menu.addItemWithTitle_action_keyEquivalent(
                &NSString::from_str(label),
                Some(action),
                ns_string!(""),
            );
            row.setTarget(Some(&actions));
        }
    }
    menu.setDelegate(Some(ProtocolObject::from_ref(&*actions)));
    item.setMenu(Some(&menu));
    ITEM.with(|cell| cell.set(item).ok());
    icon();
    dispatch2::DispatchQueue::main().exec_async(ready);
    app.run();
    anyhow::bail!("The menu bar event loop ended unexpectedly.")
}
