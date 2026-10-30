//! Marquee: a native card with the cover, title and artist of each new song
//! that starts in the Spotify desktop app.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use marquee::settings::{self, Settings};
use marquee::{UserEvent, card, pipeline, tray};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::MenuEvent;

fn main() -> anyhow::Result<()> {
    init_logging();
    let instance = single_instance::SingleInstance::new("Marquee-6f1c2a")?;
    if !instance.is_single() {
        tracing::info!("another copy is already running");
        return Ok(());
    }
    let mut settings = Settings::load();
    tracing::info!("starting with {settings:?}");

    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = menu_proxy.send_event(UserEvent::Menu(e));
    }));
    pipeline::spawn(proxy);

    let mut tray = None;
    let mut card = None;
    let mut shown_generation = 0;
    let mut test_index = 0;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                // The tray and the card need a running event loop on macOS.
                tray = tray::Tray::new(&settings)
                    .map_err(|e| tracing::error!("tray: {e:#}"))
                    .ok();
                card = card::platform_card().map_err(|e| tracing::error!("card: {e:#}")).ok();
                apply_login(settings.start_at_login);
            }
            Event::UserEvent(UserEvent::Show { content, generation }) => {
                if settings.enabled
                    && let Some(card) = card.as_mut()
                {
                    shown_generation = generation;
                    card.show(content, &settings);
                }
            }
            Event::UserEvent(UserEvent::Cover { png, generation }) => {
                if generation == shown_generation
                    && let Some(card) = card.as_mut()
                {
                    card.update_cover(png);
                }
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                let Some(tray) = tray.as_ref() else { return };
                let before = settings.clone();
                match tray.handle(e.id(), &mut settings) {
                    Some(tray::Action::Changed) => {
                        if let Err(e) = settings.save() {
                            tracing::error!("could not save settings: {e:#}");
                        }
                        if before.start_at_login != settings.start_at_login {
                            apply_login(settings.start_at_login);
                        }
                        if !settings.enabled
                            && let Some(card) = card.as_mut()
                        {
                            card.hide();
                        }
                    }
                    Some(tray::Action::TestCard) => {
                        if let Some(card) = card.as_mut() {
                            // A new generation, so a late cover of a real song
                            // cannot land on the test card.
                            shown_generation = u64::MAX - test_index;
                            card.show(pipeline::test_content(test_index), &settings);
                            test_index += 1;
                        }
                    }
                    Some(tray::Action::Quit) => *control_flow = ControlFlow::Exit,
                    None => {}
                }
            }
            _ => {}
        }
    });
}

/// Turn start at login on or off to match the setting.
fn apply_login(enabled: bool) {
    let result = (|| -> anyhow::Result<()> {
        let exe = std::env::current_exe()?;
        let launcher = auto_launch::AutoLaunchBuilder::new()
            .set_app_name("Marquee")
            .set_app_path(&exe.to_string_lossy())
            .build()?;
        match (enabled, launcher.is_enabled()?) {
            (true, false) => launcher.enable()?,
            (false, true) => launcher.disable()?,
            _ => {}
        }
        Ok(())
    })();
    if let Err(e) = result {
        tracing::error!("start at login: {e:#}");
    }
}

/// Log to a file in the config directory. The file restarts when it grows past 1 MB.
fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let path = settings::config_dir().join("marquee.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1 << 20) {
        let _ = std::fs::remove_file(&path);
    }
    let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let filter = EnvFilter::try_from_env("MARQUEE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .with_env_filter(filter)
        .init();
}
