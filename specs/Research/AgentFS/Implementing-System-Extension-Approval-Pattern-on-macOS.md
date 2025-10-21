# Implementing System Extension Approval Pattern on macOS (Sequoia & Tahoe)

**Note:** This guide assumes you are targeting macOS 15 _Sequoia_ and macOS 16 _Tahoe_ (the latest versions as of 2025). The system extension handling changed significantly in these versions. We will cover how to package your system extensions (FSKit and Endpoint Security), how to check and request their activation in your Swift app, and how to guide the user through approving them in System Settings (with deep-linking to the exact pane for convenience).

## 1\. **Create and Configure the System Extension Targets**

First, set up two separate **system extension** targets in your Xcode project (via XcodeGen configuration). One will be an **FSKit file-system module** and the other an **Endpoint Security extension**. In your project.yml (XcodeGen spec), define each extension as a target with the appropriate product type and settings:

- **FSKit Extension Target**: Use the **FSKit** framework to create a File System extension module. In the extension’s Info.plist, set the NSExtensionPointIdentifier to the FSKit extension point:

- NSExtensionPointIdentifier \= "com.apple.fskit.fsmodule"

- This identifies it as an FSKit module [\[1\]](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751#:~:text=To%20open%20the%20File%20System,Extension%20pane). Ensure you include the FSKit framework in this target and implement the necessary FSModule subclass (the class that defines your custom filesystem logic).

- **Endpoint Security Extension Target**: Set the Info.plist NSExtensionPointIdentifier to the Endpoint Security extension point. Apple’s documentation lumps Endpoint Security under system extensions (separate from drivers and network extensions)[\[2\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=There%20are%20three%20types%20of,system%20extensions%20in%20macOS%20Catalina). Typically, you’ll use something like:

- NSExtensionPointIdentifier \= "com.apple.system_extension.endpoint_security"

- (Apple’s exact string isn’t explicitly documented, but by convention it likely follows the pattern used for driver and network extensions. For instance, network extensions use "com.apple.system_extension.network_extension".) The extension code should use the EndpointSecurity framework (C API) to subscribe to events.

- **Entitlements**: Both extensions require specific entitlements:

- For the Endpoint Security extension, add the **Endpoint Security Client** entitlement (com.apple.developer.endpoint-security.client) set to **true** in its .entitlements file[\[3\]](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.endpoint-security.client#:~:text=com.apple.developer.endpoint,events%20for%20potentially%20malicious%20activity). This is mandatory to use the ES API (and note: Apple requires special permission to enable this entitlement for distribution).

- For the FSKit extension, include any FSKit-specific entitlements if needed (FSKit may not require special Apple permission, but ensure the extension has the default DriverKit entitlements like com.apple.developer.driverkit.userclient-access if applicable).

- **Signing & Capabilities**: In Xcode (or via XcodeGen settings), enable the **Hardened Runtime** for your app and extensions. Use your Developer ID or Apple development certificate to sign the extensions. The container app and extensions should all share the same Team ID. If you use App Groups or XPC between the app and extension, configure an App Group capability so both can communicate (though not strictly required just to enable the extension).

- **Embedding the Extensions**: Ensure the system extension targets are embedded in the container app. In Xcode, you’d add them under the app target’s **“Embedded Content”** (the .systemextension gets bundled into your app’s .app). With XcodeGen, you can specify embed: true for those targets under your main app target, so Xcode copies them into the app bundle. This allows the app to locate and activate them.

## 2\. **Checking and Activating the Extensions in Your Main App**

In your main Swift app, use the **SystemExtensions framework** (import SystemExtensions) to manage the extensions. The general pattern is:

- **At App Launch**: Check if the extensions are already active. You can attempt activation unconditionally; macOS will handle already-active extensions gracefully. For example, Karabiner-Elements on launch always checks/activates its driver extension so it’s running[\[4\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Karabiner,extension%20already%20allowed%20in%20EventViewer)[\[5\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Otherwise%2C%20you%20have%20to%20approve,extension%20in%20macOS%20System%20Settings). Alternatively, you might persist a flag after first successful activation, but it’s often simplest to just call activation each time and handle the result.

- **Submit Activation Request**: Create an OSSystemExtensionRequest for each extension and submit it via OSSystemExtensionManager. For example:

```
import SystemExtensions

let fsExtensionIdentifier \= "your.bundle.id.fskit-ext" // replace with your FSKit extension’s bundle ID
let esExtensionIdentifier \= "your.bundle.id.endpoint-ext" // replace with your ES extension’s bundle ID

// Create requests for each extension
let fsRequest \= OSSystemExtensionRequest.activationRequest(forExtensionWithIdentifier: fsExtensionIdentifier, queue: .main)
let esRequest \= OSSystemExtensionRequest.activationRequest(forExtensionWithIdentifier: esExtensionIdentifier, queue: .main)

// Assign a common delegate (e.g., AppDelegate or a dedicated class)
fsRequest.delegate \= self
esRequest.delegate \= self

OSSystemExtensionManager.shared.submit(fsRequest)
OSSystemExtensionManager.shared.submit(esRequest)
```

This code will ask the system to activate the extensions (install and run them)[\[6\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=You%20can%20activate%20a%20system,in%20Swift%20with%20this%20request). Ensure your app (or delegate class) conforms to OSSystemExtensionRequestDelegate so it can receive callbacks.

- **Implement the Delegate Callbacks**: The key delegate methods to implement are:

```
func request(\_ request: OSSystemExtensionRequest, didFinishWithResult result: OSSystemExtensionRequest.Result) {
    // Called when the request completes (successfully or after user approval).
    switch result {
    case .completed:
        print("Extension activated successfully.")
    case .willCompleteAfterReboot:
        print("Extension will activate after a reboot.")
        // You might inform the user a restart is needed (though on modern macOS this is rare).
    default:
        print("Extension activation finished with result: \\(result)")
    }
}
func request(\_ request: OSSystemExtensionRequest, didFailWithError error: Error) {
    print("Extension activation failed: \\(error.localizedDescription)")
    // Handle errors (e.g., missing entitlements or other failures)
}
func requestNeedsUserApproval(\_ request: OSSystemExtensionRequest) {
    print("Extension activation needs user approval.")
    // Here, we will prompt the user to approve the extension in System Settings.
    promptUserToApproveExtension(for: request)
}
func request(\_ request: OSSystemExtensionRequest,
            actionForReplacingExtension existing: OSSystemExtensionProperties,
            withExtension ext: OSSystemExtensionProperties) \-\> OSSystemExtensionRequest.ReplacementAction {
    // This is invoked if an extension with the same identifier is already installed but a new version is available.
    // Typically, instruct to replace the old extension with the new one:
    return .replace
}
```

Explanation: \- requestNeedsUserApproval is called when macOS blocks the extension from activating until the user explicitly approves it[\[7\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=2,by%20a%20user%20for%20activation). **Importantly, the activation request will** wait **in the background until approval is given or the app exits**[\[8\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=The%20first%20time%20an%20extension,it%20or%20closes%20the%20application). \- didFinishWithResult is called when the activation either completes immediately (if already approved or no approval needed) or after the user approves and the extension actually loads. The result can indicate if a reboot is required (very uncommon for system extensions – usually .completed means it’s active without reboot, whereas .willCompleteAfterReboot would mean it won’t run until after a restart[\[9\]](<https://developer.apple.com/documentation/systemextensions/ossystemextensionrequestdelegate/request(_:didfinishwithresult:)#:~:text=Documentation%20developer,until%20after%20the%20next%20restart>)). \- The replacement delegate allows you to handle upgrades: returning .replace ensures that if the user installed a new app version with an updated extension, the old one is replaced seamlessly[\[10\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=3,the%20feature%20was%20previously%20activated).

- **Detecting Already-Active Extensions**: If an extension was already approved and active from a previous run, the OSSystemExtensionManager may _not_ call requestNeedsUserApproval at all; it might directly call didFinishWithResult(.completed) or simply finish silently. In practice, you’ll know the extension is active if no error and no needsUserApproval occurred. You can also verify by trying to communicate with the extension (for example, establishing an XPC connection if your extension provides one). Karabiner, for instance, checks its extension status via a utility (EventViewer) that lists whether the driver extension is “activated \[enabled\]”[\[4\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Karabiner,extension%20already%20allowed%20in%20EventViewer). You could similarly use Apple’s systemextensionsctl in Terminal during development to list extensions: e.g. systemextensionsctl list will show entries for com.apple.system_extension.endpoint_security or FSKit, with status "enabled active" if running[\[11\]](https://knowledge.broadcom.com/external/article/291372/unable-to-upgrade-or-install-due-to-exis.html#:~:text=Unable%20to%20upgrade%20or%20install,state%5D).

## 3\. **Prompting the User to Approve the Extensions**

User approval is required the first time a system extension is installed (for security, macOS wants user consent for these powerful extensions). To provide the best UX (similar to Karabiner-Elements and other mature apps), guide the user step-by-step:

- **Custom Alert or Onboarding Screen**: When your delegate’s requestNeedsUserApproval(\_:) is called, present a clear message to the user. For example:

“**Action Required:** To enable full functionality, please allow the system extensions for MyApp. Click ‘Open Settings’ and enable both the File System Extension and Endpoint Security Extension for MyApp.”

Include an **“Open System Settings”** button in this alert. This button will programmatically navigate the user directly to the right place in System Settings.

- **Navigate to the Correct System Settings Pane**: Apple provides special URL schemes (x-apple.systempreferences:) to open specific panes in System Settings. We can take the user **directly to the Extensions approval screen**:

- For **macOS 15+ (Sequoia and Tahoe)**: You can deep-link to the **Extensions** category and even the specific sub-section:
  - **FSKit extension pane**: Use the URL for the File System Extensions section:

  - NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.ExtensionsPreferences?extensionPointIdentifier=com.apple.fskit.fsmodule")\!)

  - This opens System Settings _directly to the “File System Extension” approval pane_[\[1\]](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751#:~:text=To%20open%20the%20File%20System,Extension%20pane), where your FSKit module will be listed.

  - **Endpoint Security extension pane**: Use the URL for Endpoint Security Extensions:

  - NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.ExtensionsPreferences?extensionPointIdentifier=com.apple.system_extension.endpoint_security.extension-point")\!)

  - This should navigate to the “Endpoint Security Extensions” section (similar to how the driver/network extension identifiers work). In macOS 15, **System Settings \> General \> Login Items & Extensions** has a subcategory for "Endpoint Security Extensions"[\[12\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Endpoint%20Security%20Extensions), and your extension will appear there. _(Note: The exact extensionPointIdentifier for endpoint security is inferred; Apple’s UI categorizes it under Endpoint Security, as confirmed by Apple’s docs and MDM guidance[\[13\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Drivers)[\[12\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Endpoint%20Security%20Extensions).)_

  - **Driver extensions (if any)**: For reference, a driver extension can be opened via ...extensionPointIdentifier=com.apple.system_extension.driver_extension.extension-point. (This was reported to work on macOS 15.0[\[14\]](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751#:~:text=%40buddax2%20%40dilames%20thank%20you%20guys,0). Newer macOS versions may open the category slightly differently, but since FSKit and ES are our focus, driver extension is just noted here for completeness.)

- Using these URLs will pop open System Settings at the precise section, saving the user from hunting through menus.

- For **macOS 14 (Sonoma) or earlier**: If your app ever runs on older macOS, the above direct links won’t work (those OS still used the old “Security & Privacy” pane for system extensions). In that case, you should fall back to opening the Privacy & Security settings:

- NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension")\!)

- This opens **Privacy & Security** in System Settings[\[15\]](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751#:~:text=As%20I%20know%2C%20use%20%60x,parameter%20to%20open%20details%20window) and scrolls to the section where a “**Allow**” or “**Details...**” button for the blocked extension will appear. The user would then click “Allow”/“Details” and follow prompts (including possibly entering their Mac password) to enable the extension.
  **However**, since we target Sequoia/Tahoe, you’ll likely focus on the new Extensions interface, but it’s wise to handle this for compatibility or at least document it for users on older macOS.

- **User Action in System Settings**: Explain to the user what to do in the Settings pane:

- In macOS 15+, after clicking your “Open Settings” button, the user will see **Login Items & Extensions \> Endpoint Security Extensions / File System Extensions** with your extensions listed (likely with a toggle or an “Allow” switch). They should click the **“(i)” info button** next to the relevant category if needed, then **enable** the extension. For example, Apple’s documentation says: _“click the Info button next to an option, turn the option on, then click Done”_ for enabling extensions[\[16\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Extensions%20settings)[\[17\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Added%20extensions). In the “Endpoint Security Extensions” section, the user would toggle on your extension (and may be prompted to enter their password to confirm). In the “File System Extension” section, similarly enable your FSKit module.

- In macOS 14 or earlier, the user would find an “Allow \[Developer\]” button in Privacy & Security. On clicking it, they might have to authenticate, then the system will load the extension.

- **Resume App Flow After Approval**: Once the user has approved, macOS will launch your extension processes. If your app remained open during this, the **didFinishWithResult** delegate should fire indicating success[\[18\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=After%20the%20first%20launch%2C%20a,Result%29%20callback). At this point, you can dismiss any waiting UI in your app. For instance, Karabiner-Elements’ setup waits until the driver shows as “activated” then allows the user to proceed[\[4\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Karabiner,extension%20already%20allowed%20in%20EventViewer)[\[5\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Otherwise%2C%20you%20have%20to%20approve,extension%20in%20macOS%20System%20Settings). You might poll or simply rely on the delegate callback. If the user took a long time to approve and your delegate wasn’t called (there have been reports that the callback might not always fire if approval happens much later[\[8\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=The%20first%20time%20an%20extension,it%20or%20closes%20the%20application)), you can also detect activation by attempting to use the extension’s functionality (e.g. mount a test filesystem for FSKit, or call an XPC method on the ES extension). On success, you know it’s active.

- **Error Handling**: If the user **denies** the extension or closes System Settings without enabling it, your extension will remain in a “waiting for approval” state. You should handle this by periodically reminding the user or re-triggering the approval prompt next time the app launches. (macOS will keep the request pending; the user can still go enable it later without a new request.) Provide a clear path in your UI like “Retry Extension Activation” which again calls submitRequest and shows the prompt.

## 4\. **Completing Setup and Using the Extensions**

After approval, your extensions should be up and running: \- The FSKit module will be loaded and can register the new filesystem type (at this point you can attempt to mount or use whatever functionality your FSKit module provides). \- The Endpoint Security extension process will be running (you can confirm via Activity Monitor or systemextensionsctl), ready to monitor system events. Typically, the ES extension will establish a client connection to the Endpoint Security framework (es_new_client) and possibly communicate with your main app. If you need to pass data between the app and extension (for example, to tell the extension what events to monitor or to report events to the UI), set up an XPC listener in the extension and connect to it from the app.

Karabiner-Elements, for example, runs a background daemon (karabiner_grabber) and uses the virtual HID device driver extension. They also require Input Monitoring permission for the app to intercept keystrokes[\[19\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Enable%20Input%20Monitoring). In your case, if your app or extensions need additional permissions (like Full Disk Access for ES to inspect file events, or Input Monitoring if intercepting input), be sure to prompt for those as well. You might incorporate those into your initial setup flow (each with its own guidance).

Finally, you can provide a UI in your app (like an “About Extensions” or “Check System Extensions Status” screen) to show the status of your extensions. For instance, list whether each extension is “✅ Enabled” or “❌ Not enabled” based on either an API call or the last known state. Karabiner’s EventViewer shows the status “\[activated enabled\]” for its driver extension when it’s allowed[\[4\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Karabiner,extension%20already%20allowed%20in%20EventViewer). You could achieve something similar by maintaining state or querying the OS.

## 5\. **Summary of the User-Friendly Flow**

To summarize the pattern:

1. **App Launch**: Submit activation requests for required system extensions using OSSystemExtensionManager[\[6\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=You%20can%20activate%20a%20system,in%20Swift%20with%20this%20request).

2. **If Already Approved**: Extensions activate silently in the background. Your app proceeds knowing the extensions are running.

3. **If Approval Needed**: Your delegate’s requestNeedsUserApproval triggers, and you pause normal app operation to prompt the user.

4. **Guide User to Approve**: Show an alert explaining why the extension is needed and how to enable it. Offer a one-click “Open System Settings” button that uses the x-apple.systempreferences: URL to navigate **precisely to the Extensions pane** (File System Extensions or Endpoint Security Extensions)[\[1\]](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751#:~:text=To%20open%20the%20File%20System,Extension%20pane)[\[12\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Endpoint%20Security%20Extensions). This replicates the convenient experience apps like Karabiner provide.

5. **User Enables Extension**: The user toggles your extension on (and authenticates if required) in System Settings (e.g. under _Login Items & Extensions \> Endpoint Security Extensions_ for an ES extension)[\[12\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Endpoint%20Security%20Extensions). The extension is now allowed.

6. **Extension Activates**: macOS launches the extension. The app receives a callback didFinishWithResult(.completed) indicating success, or you detect the extension is now running (e.g., via XPC connection).

7. **Proceed**: Dismiss the setup prompt and continue normal operation, now with the extensions providing their functionality. For example, the FSKit filesystem can be mounted/used and the Endpoint Security extension will start feeding security events to your app.

8. **Persist & Update**: Remember that once approved, the user shouldn’t be prompted again on next app launch. But if your app updates the extension (new version), the SystemExtensions framework will call actionForReplacingExtension – handle it to replace the old one[\[10\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=3,the%20feature%20was%20previously%20activated). The user might not need to re-approve minor version updates, as long as the Team ID and bundle ID remain the same and the extension was already approved.

By following these steps, your app will provide a seamless, **user-friendly onboarding** for the system extensions. This pattern – **check extensions \-\> request activation \-\> prompt user \-\> deep-link to settings \-\> wait for approval \-\> continue** – is exactly what mature apps like Karabiner-Elements do to deal with macOS’s security requirements while minimizing user confusion.

**References:**

- Apple Developer Documentation: _Installing System Extensions and Drivers_ (SystemExtensions framework usage) – including OSSystemExtensionRequest and delegate methods[\[7\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=2,by%20a%20user%20for%20activation)[\[18\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=After%20the%20first%20launch%2C%20a,Result%29%20callback).

- Karabiner-Elements Documentation: _Required macOS settings_ – example of guiding users to enable the DriverKit extension under System Settings[\[5\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Otherwise%2C%20you%20have%20to%20approve,extension%20in%20macOS%20System%20Settings)[\[20\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Approve%20system%20extension).

- Apple Support Documentation: _Login Items & Extensions settings on Mac_ – explains the new Extensions UI in System Settings (categories like **Driver Extensions**, **Endpoint Security Extensions**, etc.)[\[13\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Drivers)[\[12\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Endpoint%20Security%20Extensions).

- GitHub Gist (rmcdongit): _System Preferences URL Schemes_ – provides the special x-apple.systempreferences URLs for opening specific panes (used for our deep links to the Extensions sections)[\[1\]](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751#:~:text=To%20open%20the%20File%20System,Extension%20pane)[\[15\]](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751#:~:text=As%20I%20know%2C%20use%20%60x,parameter%20to%20open%20details%20window).

---

[\[1\]](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751#:~:text=To%20open%20the%20File%20System,Extension%20pane) [\[14\]](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751#:~:text=%40buddax2%20%40dilames%20thank%20you%20guys,0) [\[15\]](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751#:~:text=As%20I%20know%2C%20use%20%60x,parameter%20to%20open%20details%20window) Apple System Preferences URL Schemes · GitHub

[https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751](https://gist.github.com/rmcdongit/f66ff91e0dad78d4d6346a75ded4b751)

[\[2\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=There%20are%20three%20types%20of,system%20extensions%20in%20macOS%20Catalina) [\[6\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=You%20can%20activate%20a%20system,in%20Swift%20with%20this%20request) [\[7\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=2,by%20a%20user%20for%20activation) [\[8\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=The%20first%20time%20an%20extension,it%20or%20closes%20the%20application) [\[10\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=3,the%20feature%20was%20previously%20activated) [\[18\]](https://www.apriorit.com/dev-blog/669-mac-system-extensions#:~:text=After%20the%20first%20launch%2C%20a,Result%29%20callback) System Extensions and DriverKit instead of Kernel Development \- Apriorit

[https://www.apriorit.com/dev-blog/669-mac-system-extensions](https://www.apriorit.com/dev-blog/669-mac-system-extensions)

[\[3\]](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.endpoint-security.client#:~:text=com.apple.developer.endpoint,events%20for%20potentially%20malicious%20activity) com.apple.developer.endpoint-security.client

[https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.endpoint-security.client](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.endpoint-security.client)

[\[4\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Karabiner,extension%20already%20allowed%20in%20EventViewer) [\[5\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Otherwise%2C%20you%20have%20to%20approve,extension%20in%20macOS%20System%20Settings) [\[19\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Enable%20Input%20Monitoring) [\[20\]](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/#:~:text=Approve%20system%20extension) Required macOS settings | Karabiner-Elements

[https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/](https://karabiner-elements.pqrs.org/docs/manual/misc/required-macos-settings/)

[\[9\]](<https://developer.apple.com/documentation/systemextensions/ossystemextensionrequestdelegate/request(_:didfinishwithresult:)#:~:text=Documentation%20developer,until%20after%20the%20next%20restart>) request(\_:didFinishWithResult:) | Apple Developer Documentation

[https://developer.apple.com/documentation/systemextensions/ossystemextensionrequestdelegate/request(\_:didfinishwithresult:)](<https://developer.apple.com/documentation/systemextensions/ossystemextensionrequestdelegate/request(_:didfinishwithresult:)>)

[\[11\]](https://knowledge.broadcom.com/external/article/291372/unable-to-upgrade-or-install-due-to-exis.html#:~:text=Unable%20to%20upgrade%20or%20install,state%5D) Unable to upgrade or install due to existing system extension (macOS)

[https://knowledge.broadcom.com/external/article/291372/unable-to-upgrade-or-install-due-to-exis.html](https://knowledge.broadcom.com/external/article/291372/unable-to-upgrade-or-install-due-to-exis.html)

[\[12\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Endpoint%20Security%20Extensions) [\[13\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Drivers) [\[16\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Extensions%20settings) [\[17\]](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac#:~:text=Added%20extensions) Change Login Items & Extensions settings on Mac \- Apple Support

[https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac](https://support.apple.com/guide/mac-help/change-login-items-extensions-settings-mtusr003/mac)
