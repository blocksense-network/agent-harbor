# Implementing Cross-Platform Native Notifications in Rust with Full Feature Support

This updated guide addresses the limitation of `notify-rust` not supporting actions (e.g., interactive buttons like "View Details") on macOS, while providing them on Linux and Windows. To meet all requirements from the Agent Harbor GUI specification— including native task completion notifications with titles, bodies, icons, actions, timeouts, sounds, priorities/hints, and badges where applicable—we'll use a hybrid approach:

- **Base Library: notify-rust** (v4.11.7 as of September 2025): Well-maintained (active GitHub, 10M+ downloads, regular updates), handles cross-platform basics and actions on Linux/Windows. Uses backends like zbus (Linux), mac-notification-sys (macOS; limited features), and winrt-notification (Windows).
- **macOS Supplement: objc2-user-notifications** (v0.3.1): For full macOS support, including actions, badges, and custom categories. This crate is part of the well-maintained objc2 ecosystem (frequent releases, 17M+ downloads for objc2).
- **Why Multiple Libraries?**: notify-rust provides a unified API for simplicity, but macOS requires direct framework bindings for unsupported features like actions. This ensures consistency while achieving full parity.
- **Maintenance Notes**: Both are active; notify-rust's macOS support is a subset (no actions, hints, or resident notifications), so objc2 fills the gap without redundancy on other platforms.

Use conditional compilation (`#[cfg(target_os = "...")]`) to isolate platform-specific code. Test on each OS, as user settings can override behaviors.

## Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
notify-rust = "4.11"

[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.5" # Or latest
objc2-foundation = { version = "0.2", features = ["NSObject", "NSString", "NSDictionary", "NSNumber"] }
objc2-user-notifications = "0.3"
```

## Setup and Permissions

- **Cross-Platform (notify-rust)**: No setup needed; handles backends automatically.
- **macOS-Specific**: Request permissions for alerts, sounds, and badges. App must be signed (use `codesign` for dev). Define categories for actions once on launch.
- **Permissions Handling**:
  - Linux/Windows: None required.
  - macOS: Request via `UNUserNotificationCenter` (shown below).
- **Error Handling**: Use `Result` for all calls.

### macOS Permission and Category Setup

Call this once on app launch:

```rust
#[cfg(target_os = "macos")]
use objc2::rc::autoreleasepool;
#[cfg(target_os = "macos")]
use objc2_foundation::{ns_string, NSObjectProtocol, NSString, NSMutableDictionary, NSNumber};
#[cfg(target_os = "macos")]
use objc2_user_notifications::{
    UNMutableNotificationContent, UNNotificationAction, UNNotificationCategory,
    UNNotificationRequest, UNTimeIntervalNotificationTrigger, UNUserNotificationCenter,
    UNAuthorizationOptions, UNNotificationPresentationOptions, UNNotificationSound,
};

#[cfg(target_os = "macos")]
fn setup_mac_notifications() {
    autoreleasepool(|_| {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let options = UNAuthorizationOptions::UNAuthorizationOptionAlert
            | UNAuthorizationOptions::UNAuthorizationOptionSound
            | UNAuthorizationOptions::UNAuthorizationOptionBadge;
        center.requestAuthorizationWithOptions_completionHandler(options, |granted, _err| {
            if !granted {
                eprintln!("macOS notification permissions denied");
            }
        });

        // Define category for actions
        let view_action = UNNotificationAction::actionWithIdentifier_title_options(
            ns_string!("view"),
            ns_string!("View Details"),
            0,
        );
        let category = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
            ns_string!("taskComplete"),
            &[view_action],
            &[],
            0,
        );
        center.setNotificationCategories(&[category]);

        // Set delegate for handling actions (implement UNUserNotificationCenterDelegate)
        // Example: Create a custom NSObject subclass and set center.set_delegate(&delegate);
    });
}
```

## Implementation Example

A task completion notification with title, body, icon, timeout (5 seconds), sound, and action ("View Details"). On macOS, use objc2 for full support; elsewhere, notify-rust.

```rust
use notify_rust::{Notification, Timeout, Hint};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(target_os = "macos")]
    setup_mac_notifications(); // Call once on launch

    send_task_complete_notification()?;

    Ok(())
}

fn send_task_complete_notification() -> Result<(), Box<dyn Error>> {
    #[cfg(not(target_os = "macos"))]
    {
        let mut notif = Notification::new()
            .summary("Task Completed")
            .body("Your agent workflow has finished successfully.")
            .icon("/path/to/icon.png")
            .appname("AgentHarbor")
            .sound("default") // Linux/Windows
            .timeout(Timeout::Milliseconds(5000))
            .action("view", "View Details"); // Actions on Linux/Windows

        #[cfg(target_os = "linux")]
        notif = notif.hint(Hint::Urgency(notify_rust::Urgency::Normal));

        #[cfg(target_os = "windows")]
        notif = notif.hint(Hint::Priority(notify_rust::Priority::High)); // If supported

        notif.show()?;
    }

    #[cfg(target_os = "macos")]
    {
        autoreleasepool(|_| {
            let center = UNUserNotificationCenter::currentNotificationCenter();

            let content = UNMutableNotificationContent::new();
            unsafe {
                content.set_title(ns_string!("Task Completed"));
                content.set_body(ns_string!("Your agent workflow has finished successfully."));
                content.set_sound(&UNNotificationSound::defaultSound());
                content.set_badge(&NSNumber::numberWithInteger(1)); // Badge
                content.set_categoryIdentifier(ns_string!("taskComplete")); // For actions
                let user_info = NSMutableDictionary::dictionary();
                user_info.setObject_forKey(ns_string!("123"), ns_string!("taskId"));
                content.set_userInfo(&user_info);
            }

            let trigger = UNTimeIntervalNotificationTrigger::triggerWithTimeInterval_repeats(1.0, false); // Immediate

            let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
                ns_string!("taskCompleteRequest"),
                &content,
                Some(&trigger),
            );
            center.addNotificationRequest_withCompletionHandler(&request, |_err| {});
        });
    }

    Ok(())
}
```

- **Handling Actions**:
  - **Linux/Windows (notify-rust)**: Use `let handle = notif.show()?; handle.on_action(|action| { if action == "view" { /* Handle */ } });`.
  - **macOS (objc2)**: Implement `UNUserNotificationCenterDelegate` methods like `didReceive` to process `response.actionIdentifier` and `userInfo`.
- **Additional Features**:
  - Icons/Images: Use `.icon()` (all platforms) or macOS attachments for richer media.
  - Badges: macOS via `.set_badge`; not directly in notify-rust (use platform APIs if needed).
  - Sounds/Hints: Platform-specific as shown.

This setup delivers full requirements: actions everywhere, native integration, and minimal code duplication.
