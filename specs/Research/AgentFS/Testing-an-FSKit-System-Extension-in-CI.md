# Testing an FSKit System Extension in CI (macOS)

## Preparing the CI Environment for Unsigned System Extensions

To run integration tests on an **FSKit-based file system extension** in a headless CI (e.g. GitHub Actions), you need to relax macOS security to allow loading an unsigned system extension. In practice, this means:

- **Disable System Integrity Protection (SIP)** on the test machine. SIP must be turned off from macOS Recovery (using csrutil disable) before you can load unsigned system extensions[\[1\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=In%20theory%2C%20disabling%20SIP%20should,the%20entitlements%20can%20be%20embedded). (GitHub’s hosted runners have SIP enabled by default, so you may need a self-hosted macOS runner or a custom VM with SIP off.)

- **Enable System Extension Developer Mode:** Run sudo systemextensionsctl developer on to put the system in developer mode[\[1\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=In%20theory%2C%20disabling%20SIP%20should,the%20entitlements%20can%20be%20embedded). Developer mode skips version checks so you can reload a new extension build without bumping the version each time[\[2\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=When%20the%20computer%20examines%20a,this%20version%20check%20is%20skipped). It also allows loading extensions from outside the usual locations and helps if you are loading the extension without a full installer or app GUI[\[1\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=In%20theory%2C%20disabling%20SIP%20should,the%20entitlements%20can%20be%20embedded). (Enabling this requires SIP to be off; otherwise you’ll get an error prompting to disable SIP[\[3\]](https://developer.apple.com/forums/thread/663135#:~:text=Extensions,telling%20me%20to%20disable%20SIP).)

- **(Optional) Disable code-signing enforcement:** If your extension binary is completely unsigned (no code signature at all or missing required entitlements), you may need to disable Apple’s code signing checks. This can be done by setting an NVRAM boot-arg to turn off the Apple Mobile File Integrity (AMFI) protections. Reboot into Recovery and run: nvram boot-args="amfi_get_out_of_my_way=0x1". According to Apple’s guidance, this step is only needed if you don’t have a proper development code signature with the required entitlements for the extension[\[4\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=As%20per%20Eskimo%20answer%20on,Forums%20you%20might%20additionally%20to). In many cases, using a **free “Mac Developer” certificate** to sign the extension with the correct entitlements (e.g. the File Provider/File System Extension entitlement) is preferable to fully disabling AMFI[\[1\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=In%20theory%2C%20disabling%20SIP%20should,the%20entitlements%20can%20be%20embedded).

**Note:** The above steps effectively put the system in an insecure, developer-friendly state. On such a system, macOS will be more permissive in loading your FSKit extension for testing. For example, with SIP off and developer mode on, the extension can be loaded without being notarized or formally approved by the user[\[1\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=In%20theory%2C%20disabling%20SIP%20should,the%20entitlements%20can%20be%20embedded). (If these steps are not done, macOS will refuse to load the extension with an “OSSystemExtensionErrorDomain error 8” or **“invalid signature”** error[\[5\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=OSSystemExtensionErrorDomain%20error%208)[\[6\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=,extension%20regardless%20I%20disable%20SIP).)

## Loading the FSKit System Extension in Headless Mode

Once the machine is prepared, you can **activate/load your FSKit extension** in CI. Typically, FSKit file systems are delivered as a System Extension inside a container app (an .app with an .appex). In a GUI environment, you would launch the app and approve the extension in **System Settings \> Privacy & Security**, but in CI we must automate this:

1. **Install or register the extension:** Ensure the built app/extension is present on the test machine. For example, you might copy your .app (containing the FSKit extension) into /Applications or /Library/SystemExtensions. This makes it available for the system to examine.

2. **Activate via developer mode:** With developer mode on, you can skip the usual user-approval prompt. In many cases, simply attempting to mount the filesystem (next step) will trigger the system to load the extension. You can verify the extension’s status by running systemextensionsctl list. If everything is configured correctly, you should see your extension listed as **“activated enabled”** (meaning it’s approved and running)[\[7\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=list%20%20%20%20List,States%20include). If it shows **“activated waiting for user”**, then macOS is still expecting a user approval[\[8\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=Available%20for%20use), which is a problem in a headless environment.

3. _Tip:_ In a CI scenario, if you have a Team ID (from a Developer certificate), you can pre-approve the extension to avoid any “waiting for user” state. This is done by installing a Configuration Profile that whitelists your Team Identifier and the extension type. The profile can allow all system extensions signed by your Team ID and mark them as approved without user interaction[\[9\]](https://www.ibm.com/docs/en/maas360?topic=settings-system-extensions#:~:text=Allow%20Users%20to%20Approve%20system,MaaS360%20loads%20all%20of%20the)[\[10\]](https://www.ibm.com/docs/en/maas360?topic=settings-system-extensions#:~:text=Allowed%20Team%20Identifier%20A%20unique,developers). You would generate a .mobileconfig with the **Allowed Team Identifiers** (and optionally the specific Bundle ID and extension type for file providers) and install it via the profiles command before loading the extension. This step isn’t required if you’ve disabled SIP & AMFI, but it’s a more secure approach if you prefer to keep SIP on (assuming you sign the extension with a certificate).

4. **(Re)Load updated builds:** Because each commit produces a new extension binary (likely with the same version), developer mode is essential. With systemextensionsctl developer on, macOS will **skip the version check** and allow reinstalling the extension even if the version string hasn’t changed[\[2\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=When%20the%20computer%20examines%20a,this%20version%20check%20is%20skipped). This means you won’t have to constantly increment the version for testing purposes – the new build will replace the old one when you load it. If you encounter issues where an old extension is still cached, you can force removal by running sudo systemextensionsctl reset to clear all system extensions (this will uninstall all extensions and reset their state)[\[11\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=Will%20be%20removed%20at%20the,next%20computer%20restart). Typically, on a fresh CI machine this isn’t necessary, but it’s useful for iterating locally.

## Mounting the Custom File System for Testing

After the extension is enabled (or in parallel with its activation), you can mount your custom filesystem in a headless manner using the standard mount command. **FSKit** integrates with the system mount utility. The general syntax is:

\# Create a mount point directory for the filesystem:  
mkdir \-p /path/to/mountpoint

\# Mount the FSKit filesystem:  
sudo mount \-F \-t \<YourFSType\> \<device\> /path/to/mountpoint \-o \<options\>

Here’s a breakdown of the command:

- \-F \-t \<YourFSType\> specifies the **filesystem type**. This should match the FS name your extension provides (for example, in the sample it’s "MyFS"; use your FS’s identifier). The \-F flag tells mount to use the new **FSKit user-space mounting** mechanism for a File System Extension[\[12\]](https://github.com/KhaosT/FSKitSample#:~:text=mkdir%20%2Ftmp%2FTestVol%20mount%20,MyFS%20disk18%20%2Ftmp%2FTestVol).

- \<device\> is a device or image that your filesystem will overlay or use. For many filesystems this is a block device (like a disk image or volume). If your filesystem is an overlay that doesn’t use a real disk device, you still need to provide _something_ for the mount command. You have a couple of options:

- If your FS extension treats an underlying block device as the backing store, provide the appropriate device node (e.g. an APFS volume’s device if you are overlaying the system disk). **Be very careful** if mounting over a live filesystem – typically you’d use a read-only backing or a snapshot to avoid corruption.

- If you just need a dummy device (for testing purposes or because your FS doesn’t actually require a real disk), you can create a temporary dummy disk image. For example: mkfile \-n 100m dummy.img will create a 100 MB file, then hdiutil attach \-imagekey diskimage-class=CRawDiskImage \-nomount dummy.img will attach it as a raw disk device without mounting it (you’ll get a device like /dev/diskN)[\[13\]](https://github.com/KhaosT/FSKitSample#:~:text=To%20create%20a%20dummy%20block,you%20can%20do%20the%20following). You can use that device in the mount command (e.g. /dev/diskN). This dummy device acts as a placeholder that your FS can work with (perhaps treating it as an empty volume to overlay on).

- In some overlay FS designs, the “device” string could also be repurposed to pass a path. (For instance, an overlay that stacks on a directory might accept the base directory path as the device argument.) Check your FS implementation — if it doesn’t interpret the device string in a special way, using a dummy disk as above is the safest approach.

- /path/to/mountpoint is the directory where you want the filesystem mounted. Make sure this directory exists beforehand (FSKit requires the mount point to be pre-created)[\[14\]](https://github.com/macfuse/macfuse/wiki/FUSE-Backends#:~:text=,This%20might%20be%20a%20FSKit).

- \-o \<options\> allows you to pass **mount options or startup parameters** to your filesystem. Any key=value pairs here will be forwarded to your FSKit volume’s mount handler. For example, you might pass something like \-o lower=/some/dir,upper=/tmp/overlay if your filesystem expects parameters for an overlay’s lower and upper directories (the exact syntax depends on your implementation). These options are accessible in your extension’s code (via the FSVolume mount options in FSKit) to configure the filesystem at startup. If your FS requires certain flags or modes, include them here. _(Note: FSKit in macOS 15.4+ does not yet support all traditional mount options by default[\[15\]](https://github.com/macfuse/macfuse/wiki/FUSE-Backends#:~:text=,using%20the%20kernel%20extension%20backend), but custom options specific to your FS should be received by your extension’s mount(options:replyHandler:) callback.)_

For example, if your FS type is OverlayFS and you’ve attached a dummy device /dev/diskN, you could do:

sudo mkdir \-p /tmp/test_mount  
sudo mount \-F \-t OverlayFS /dev/diskN /tmp/test_mount \-o lower=/data/base,upper=/data/changes

This would instruct the system to load your “OverlayFS” extension (if not already loaded) and mount it at /tmp/test_mount using /dev/diskN as a backing store, with custom parameters for the lower and upper directories.

Remember that mounting will **launch your FS extension** if it’s not already running. Thanks to developer mode, this can happen without any GUI confirmation. If the mount command fails and systemextensionsctl list shows the extension **“waiting for user”**, it means macOS didn’t auto-approve it. In that case, double-check that SIP is off and developer mode is on. You may also need to ensure the extension’s Team ID is allowed (or use the NVRAM boot-arg method described above if completely unsigned).

## Running Tests and Teardown

Once mounted, your custom filesystem is active at the specified mountpoint. You can run your integration test suite against it (file operations, etc.). This will be fully headless – the extension runs in user-space and your test process can interact with the mounted filesystem like any other volume.

After tests complete, **unmount the filesystem** to clean up. Use the standard umount (or diskutil unmount):

sudo umount /path/to/mountpoint

This will shut down the FSKit volume (and your extension instance will typically terminate if no other mounts are active)[\[12\]](https://github.com/KhaosT/FSKitSample#:~:text=mkdir%20%2Ftmp%2FTestVol%20mount%20,MyFS%20disk18%20%2Ftmp%2FTestVol). If you created a dummy disk image, you can detach it with hdiutil detach /dev/diskN as well.

Finally, if you want to remove the system extension (for example, to test a fresh install next time on a persistent runner), you can uninstall it. The systemextensionsctl reset command will remove **all** system extensions and reset their approval state[\[11\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=Will%20be%20removed%20at%20the,next%20computer%20restart). Alternatively, systemextensionsctl list will show the identifier and team ID, which you can use with systemextensionsctl uninstall \<teamID\> \<bundleID\> to remove just your extension (or you can simply delete the container app from /Applications and reboot, which also unloads it).

## Summary of Key Commands

For quick reference, here are the specific commands and steps to integrate into your CI script (with appropriate privileges):

1. **Prepare system (one-time setup on runner)** – disable SIP and enable dev mode:

2. Reboot to Recovery and run in Terminal: csrutil disable (then reboot back to macOS).

3. Enable developer mode for system extensions:

- sudo systemextensionsctl developer on

- _(Only needed once per machine; persists across reboots)_[\[1\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=In%20theory%2C%20disabling%20SIP%20should,the%20entitlements%20can%20be%20embedded)

4. _(Optional)_ Set boot-args to disable code signing enforcement (if not signing the extension at all):

- sudo nvram boot-args="amfi_get_out_of_my_way=0x1"

- _(Reboot for this to take effect)_[\[4\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=As%20per%20Eskimo%20answer%20on,Forums%20you%20might%20additionally%20to)

5. **Install and load the FSKit extension (each test run)**:

6. Install/copy the built app containing your extension to /Applications (or another appropriate location).

7. (If using a config profile to auto-approve, install that via sudo profiles \-I \-F /path/to/whitelist.mobileconfig here.)

8. Ensure developer mode is ON (in case the machine was rebooted): sudo systemextensionsctl developer on.

9. You can check status (optional): systemextensionsctl list – to see if the extension is listed.

10. **Mount the filesystem** (this triggers loading if not already loaded):

- sudo mkdir \-p /tmp/fs_test_mount  
  sudo mount \-F \-t \<YourFSType\> \<device\> /tmp/fs_test_mount \-o \<options\>

- _(Replace \<YourFSType\> with your FS name, and supply the appropriate device or dummy disk and any \-o options needed.)_[\[12\]](https://github.com/KhaosT/FSKitSample#:~:text=mkdir%20%2Ftmp%2FTestVol%20mount%20,MyFS%20disk18%20%2Ftmp%2FTestVol)[\[13\]](https://github.com/KhaosT/FSKitSample#:~:text=To%20create%20a%20dummy%20block,you%20can%20do%20the%20following)

11. **Run your integration tests** against the mounted path (read/write files, etc. as needed).

12. **Teardown**:

13. Unmount the filesystem: sudo umount /tmp/fs_test_mount[\[12\]](https://github.com/KhaosT/FSKitSample#:~:text=mkdir%20%2Ftmp%2FTestVol%20mount%20,MyFS%20disk18%20%2Ftmp%2FTestVol).

14. Optionally, detach any dummy disk: hdiutil detach /dev/diskN.

15. Optionally, remove the extension if you need a clean slate: sudo systemextensionsctl reset[\[11\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=Will%20be%20removed%20at%20the,next%20computer%20restart) (or leave it installed for next run, which is fine if developer mode is on).

By following the above steps, you can automate in-depth integration tests of your FSKit overlay filesystem on macOS in a CI pipeline. The key is to run the CI on a Mac environment configured for development (or reduced security) so that the unsigned system extension can be loaded without manual intervention[\[1\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=In%20theory%2C%20disabling%20SIP%20should,the%20entitlements%20can%20be%20embedded). Once that hurdle is cleared, mounting and exercising the filesystem can be done entirely via command-line, allowing your tests to run headlessly.

**Sources:**

- Stack Overflow – Running unsigned system extensions in macOS (disabling SIP, enabling developer mode)[\[1\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=In%20theory%2C%20disabling%20SIP%20should,the%20entitlements%20can%20be%20embedded)[\[4\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=As%20per%20Eskimo%20answer%20on,Forums%20you%20might%20additionally%20to)

- Apple systemextensionsctl Manual – Developer mode and extension states[\[2\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=When%20the%20computer%20examines%20a,this%20version%20check%20is%20skipped)[\[7\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=list%20%20%20%20List,States%20include)[\[11\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=Will%20be%20removed%20at%20the,next%20computer%20restart)

- KhaosT’s FSKit Sample – Usage of mount \-F \-t \<FS\> to mount the filesystem and create a dummy disk device for testing[\[12\]](https://github.com/KhaosT/FSKitSample#:~:text=mkdir%20%2Ftmp%2FTestVol%20mount%20,MyFS%20disk18%20%2Ftmp%2FTestVol)[\[13\]](https://github.com/KhaosT/FSKitSample#:~:text=To%20create%20a%20dummy%20block,you%20can%20do%20the%20following)

- IBM MaaS360 MDM Documentation – System Extension policy (Team ID whitelisting for auto-approval)[\[9\]](https://www.ibm.com/docs/en/maas360?topic=settings-system-extensions#:~:text=Allow%20Users%20to%20Approve%20system,MaaS360%20loads%20all%20of%20the)[\[10\]](https://www.ibm.com/docs/en/maas360?topic=settings-system-extensions#:~:text=Allowed%20Team%20Identifier%20A%20unique,developers)

---

[\[1\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=In%20theory%2C%20disabling%20SIP%20should,the%20entitlements%20can%20be%20embedded) [\[4\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=As%20per%20Eskimo%20answer%20on,Forums%20you%20might%20additionally%20to) [\[5\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=OSSystemExtensionErrorDomain%20error%208) [\[6\]](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina#:~:text=,extension%20regardless%20I%20disable%20SIP) macos \- How to run un-signed System Extensions in OSX catalina? \- Stack Overflow

[https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina](https://stackoverflow.com/questions/60674561/how-to-run-un-signed-system-extensions-in-osx-catalina)

[\[2\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=When%20the%20computer%20examines%20a,this%20version%20check%20is%20skipped) [\[7\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=list%20%20%20%20List,States%20include) [\[8\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=Available%20for%20use) [\[11\]](https://leancrew.com/all-this/man/man8/systemextensionsctl.html#:~:text=Will%20be%20removed%20at%20the,next%20computer%20restart) systemextensionsctl(8) man page

[https://leancrew.com/all-this/man/man8/systemextensionsctl.html](https://leancrew.com/all-this/man/man8/systemextensionsctl.html)

[\[3\]](https://developer.apple.com/forums/thread/663135#:~:text=Extensions,telling%20me%20to%20disable%20SIP) Debugging system extensions for ma… | Apple Developer Forums

[https://developer.apple.com/forums/thread/663135](https://developer.apple.com/forums/thread/663135)

[\[9\]](https://www.ibm.com/docs/en/maas360?topic=settings-system-extensions#:~:text=Allow%20Users%20to%20Approve%20system,MaaS360%20loads%20all%20of%20the) [\[10\]](https://www.ibm.com/docs/en/maas360?topic=settings-system-extensions#:~:text=Allowed%20Team%20Identifier%20A%20unique,developers) System Extensions

[https://www.ibm.com/docs/en/maas360?topic=settings-system-extensions](https://www.ibm.com/docs/en/maas360?topic=settings-system-extensions)

[\[12\]](https://github.com/KhaosT/FSKitSample#:~:text=mkdir%20%2Ftmp%2FTestVol%20mount%20,MyFS%20disk18%20%2Ftmp%2FTestVol) [\[13\]](https://github.com/KhaosT/FSKitSample#:~:text=To%20create%20a%20dummy%20block,you%20can%20do%20the%20following) GitHub \- KhaosT/FSKitSample: FSKit example setup

[https://github.com/KhaosT/FSKitSample](https://github.com/KhaosT/FSKitSample)

[\[14\]](https://github.com/macfuse/macfuse/wiki/FUSE-Backends#:~:text=,This%20might%20be%20a%20FSKit) [\[15\]](https://github.com/macfuse/macfuse/wiki/FUSE-Backends#:~:text=,using%20the%20kernel%20extension%20backend) FUSE Backends · macfuse/macfuse Wiki · GitHub

[https://github.com/macfuse/macfuse/wiki/FUSE-Backends](https://github.com/macfuse/macfuse/wiki/FUSE-Backends)
