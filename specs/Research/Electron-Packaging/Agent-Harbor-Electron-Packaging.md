# **Architecting a Cross-Platform Distribution Pipeline for Electron Applications: A Definitive Guide**

## **Part I: Foundational Project Setup and Unified Build Configuration**

The successful deployment of a cross-platform Electron application hinges on a robust, maintainable, and automated build pipeline. This initial phase of the process is critical, as the architectural decisions made here will dictate the scalability and reliability of the entire distribution workflow. This section establishes a solid project foundation, justifies the selection of electron-builder as the core packaging tool, and architects a unified configuration file that will serve as the single source of truth for all target platforms: macOS, Windows, and Linux. By centralizing configuration and clearly defining how application contents are managed, this approach mitigates complexity and ensures consistency across all distributables.

### **1.1 Selecting the Right Toolchain: Electron Forge vs. electron-builder**

The Electron ecosystem offers two primary toolchains for packaging and distribution: Electron Forge and electron-builder. While both are powerful, they cater to different philosophies and levels of configuration granularity. A careful selection is paramount to meeting the specific and complex requirements of this project.

Electron Forge is positioned as an "all-in-one" toolkit designed to streamline the development-to-distribution lifecycle.1 It achieves this by bundling a collection of established, single-purpose tools, such as @electron/packager for application bundling, @electron/osx-sign for macOS code signing, and various installer-specific modules like electron-winstaller.1 This integrated approach provides an excellent out-of-the-box experience, making it ideal for projects with standard requirements or for teams looking to get started quickly. Its use of "Makers" abstracts away the underlying configuration of specific installer formats, simplifying the initial setup.1

Conversely, electron-builder presents itself as a "complete solution" with a focus on comprehensive control and an extensive array of supported target formats.3 It supports numerous targets for each platform, including the specific formats required by the user query: .pkg for macOS, MSI for Windows, and AppImage for Linux.3 This breadth of native support, combined with a highly detailed and hierarchical configuration system, provides the granular control necessary for complex deployment scenarios.

For the use case at hand—which involves packaging a macOS System Extension, creating a .pkg installer, generating a Windows MSI, and producing a Linux AppImage—electron-builder is the superior choice. The requirement to package a system extension necessitates a level of control over the macOS build process that is more directly and transparently exposed in electron-builder's configuration schema. While it is possible to configure Electron Forge's underlying makers to achieve these goals, doing so often involves more complex overrides. electron-builder's direct support for .pkg, MSI, and AppImage targets, coupled with its flexible file management and signing hooks, provides a more straightforward and powerful path to a successful cross-platform build pipeline.

### **1.2 Project Initialization and Core Dependencies**

The foundation of any Electron project is a standard Node.js package structure. The initialization process involves creating a directory, initializing a package.json file, and installing the necessary development dependencies.

The project can be scaffolded by creating a new directory and running npm init or yarn init.4 This command prompts for essential project metadata such as the package name, version, description, and author, all of which will be used by electron-builder to populate the metadata of the final application installers.3 It is crucial to define the main entry point, which should point to the JavaScript file that runs Electron's main process (e.g., main.js).4

Once the package.json is created, the core dependencies must be installed. The two essential packages for this workflow are electron and electron-builder. Both should be installed as development dependencies (devDependencies) using the command npm install electron electron-builder \--save-dev.5 The Electron package itself is classified as a devDependency because the Electron binary, which contains all the necessary runtime APIs, is bundled into the final application during the packaging phase. It is not a library that needs to be included in the production node\_modules folder, as the packager handles its inclusion directly.4

With the dependencies installed, basic scripts should be added to the scripts section of package.json to streamline development and building. A typical setup includes:

* A start script to run the application in development mode: "start": "electron.".4
* A dist script to trigger the packaging process for all configured platforms: "dist": "electron-builder".3

This simple setup provides the necessary framework to begin development and to later execute the complex, cross-platform build configurations that will be defined.

### **1.3 Architecting the Unified electron-builder.yml Configuration**

To manage the complexity of a multi-platform build, a centralized and well-structured configuration is essential. While electron-builder can be configured directly within the build key of package.json, a dedicated configuration file offers superior readability, maintainability, and separation of concerns.6 The recommended format is a YAML file named electron-builder.yml placed in the project's root directory.

electron-builder employs a clear configuration hierarchy. Settings provided via command-line interface (CLI) flags take the highest precedence, followed by the dedicated electron-builder.yml file, and finally the build object within package.json.6 By adopting the electron-builder.yml approach, the build configuration is decoupled from the project's general package metadata, which is a best practice for large or complex projects.

The electron-builder.yml file is structured with a set of top-level properties that apply globally, and platform-specific blocks for macOS, Windows, and Linux.

Top-Level Properties:
These properties define the core identity and structure of the application across all platforms.

* appId: A unique identifier for the application, typically in reverse domain-name notation (e.g., com.mycompany.myapp). This is a critical field, as it is used as the CFBundleIdentifier on macOS and the Application User Model ID (AUMID) on Windows, ensuring application identity is consistent.6
* productName: The user-facing name of the application.
* copyright: The copyright notice, often in the format Copyright © year ${author}.8
* directories: An object specifying output paths. The output key defines where the final installers will be placed (e.g., release/), and buildResources specifies the location of assets like icons and installer scripts (e.g., build/).8

Platform-Specific Blocks:
The power of the unified configuration lies in the mac, win, and linux top-level keys. These blocks contain all the settings specific to each target operating system, allowing for tailored configurations while keeping everything within a single, coherent file.6 Subsequent parts of this report will delve into the specific options required within each of these blocks.
A foundational electron-builder.yml might look like this:

YAML

appId: com.mycompany.myapp
productName: MyApp
copyright: Copyright © 2024 ${author}
directories:
  output: release/
  buildResources: build/

\# Platform-specific configurations will be added here
mac:
  \# macOS configuration
win:
  \# Windows configuration
linux:
  \# Linux configuration

This structure provides a clean and scalable foundation for managing the distinct requirements of each platform.

### **1.4 Managing Application Contents: files, extraResources, and extraFiles**

Effectively managing which files are included in the final application package is crucial for both functionality and security. electron-builder provides three primary configuration keys for this purpose: files, extraResources, and extraFiles. Each serves a distinct purpose in composing the application's contents.

The files property is an array of glob patterns that explicitly defines which files and directories should be included in the application's app.asar archive (or app directory if ASAR packing is disabled).9 electron-builder is intelligent by default; it automatically includes all production dependencies from package.json and excludes development dependencies, so there is no need to explicitly ignore node\_modules or devDependencies.3 A common practice is to start with a broad pattern like \*\*/\* and then add negative patterns to exclude source files, build scripts, or other artifacts that are not needed at runtime.

The extraResources property is used to copy files and directories into the application's resources folder. On macOS, this corresponds to the Contents/Resources directory within the .app bundle; on Windows and Linux, it is a resources directory alongside the application executable.9 This is the ideal location for platform-agnostic assets that the application needs to access at runtime, such as databases, configuration files, or pre-trained machine learning models. These files are not packed into the app.asar archive and can be accessed programmatically using the process.resourcesPath property in Electron.11

The extraFiles property, while similar to extraResources, copies files to a different location: the app's main content directory. For macOS, this is the Contents directory itself, which is the parent of Resources. For Windows and Linux, this is the root directory of the application package.9 This distinction is subtle but of immense strategic importance for advanced packaging scenarios.

The primary challenge of the user's request is the inclusion of a macOS System Extension (.systemextension). According to Apple's guidelines, a system extension must be placed within the main application's bundle at a specific path: YourApp.app/Contents/Library/SystemExtensions/.12 Using extraResources would place the extension in the incorrect Contents/Resources directory. The extraFiles property, however, provides the necessary mechanism. By defining a FileSet object within the extraFiles array, it is possible to specify a source (from) path for the pre-built .systemextension and a destination (to) path relative to the Contents directory. A configuration such as { "from": "path/to/MyExtension.systemextension", "to": "Library/SystemExtensions/MyExtension.systemextension" } will precisely inject the extension into the required location within the .app bundle before the signing and packaging stages commence.9 This technique is the critical, and often undocumented, lynchpin for successfully packaging modern macOS System Extensions with Electron. Its correct application is not merely a matter of asset management but a fundamental prerequisite for the entire macOS distribution workflow.

## **Part II: Mastering macOS Distribution with System Extensions**

The distribution of macOS applications, particularly those extending the operating system's functionality, is a uniquely complex and rigorously controlled process. Apple's security-first approach mandates a multi-stage procedure of code signing, entitlement provisioning, and notarization. For applications that include a System Extension, these requirements become even more stringent. This section provides an exhaustive, step-by-step guide to navigating this process using electron-builder, from understanding the architectural shift to modern extensions, to packaging the final .pkg installer, and successfully passing Apple's automated security checks.

### **2.1 A Primer on Modern macOS Extensions**

To comprehend the intricate packaging requirements, it is essential to first understand the evolution of macOS extensions. For years, developers extended the OS's core functionality using Kernel Extensions (.kext). These modules load directly into the macOS kernel, granting them powerful capabilities but also posing significant risks to system stability and security. A bug in a .kext could lead to a kernel panic, crashing the entire system.14

Beginning with macOS Catalina, Apple deprecated kernel extensions for most use cases and introduced System Extensions (.systemextension) as the modern replacement.14 The fundamental architectural difference is that System Extensions run in a tightly controlled user-space environment, not in the kernel.14 This isolation prevents them from compromising the integrity of the core operating system. Apple provides specific frameworks for different types of extensions, such as DriverKit for hardware drivers, NetworkExtension for VPN clients and content filters, and EndpointSecurity for antivirus and security software.15

This shift to a more secure model comes with a strict set of rules for any application that deploys a system extension. Apple mandates that such applications must be distributed via a signed installer package (.pkg), not a simple drag-and-drop disk image (.dmg).16 Furthermore, both the main application and the system extension must be signed with a valid Developer ID, provisioned with specific entitlements, and successfully notarized by Apple to run on end-user systems without security warnings.12

### **2.2 Packaging the System Extension and Creating the.pkg Installer**

The first technical challenge is to correctly structure the application bundle to include the system extension and then configure electron-builder to produce the required .pkg installer.

#### **Step 1: Placing the System Extension**

As established in Part I, the .systemextension bundle must reside at a precise location within the main application's .app bundle: Contents/Library/SystemExtensions/. This is achieved using the extraFiles configuration in electron-builder.yml. Assuming the pre-compiled MyExtension.systemextension is located in a build/extensions directory within the project, the configuration is as follows:

YAML

mac:
  \#... other mac configurations
  extraFiles:
    \- from: build/extensions/MyExtension.systemextension
      to: Library/SystemExtensions/MyExtension.systemextension

This directive instructs electron-builder to copy the extension into the correct subdirectory within the .app bundle during the packaging phase, making it available for subsequent signing and activation.9

#### **Step 2: Configuring the.pkg Target**

The default build target for macOS in electron-builder is .dmg. To comply with Apple's requirements for system extensions, this must be changed to .pkg. This is a simple but non-negotiable configuration change within the mac block:

YAML

mac:
  target:
    \- target: pkg
      arch:
        \- x64
        \- arm64

This configuration specifies that electron-builder should produce a .pkg installer as its output. It is also configured to create a universal binary supporting both Intel (x64) and Apple Silicon (arm64) architectures, which is standard practice for modern macOS applications.17

#### **Step 3: Post-Installation Scripts (If Necessary)**

While modern system extensions are typically activated programmatically from within the running application, the .pkg installer format supports powerful preinstall and postinstall scripts. electron-builder exposes this functionality through the scripts option within the pkg configuration, which points to a directory containing these scripts (by default, build/pkg-scripts).17 Although not always required for system extensions, these scripts can be invaluable for tasks such as cleaning up old versions of a kernel extension during an upgrade or performing other system-level setup tasks. Any script must be executable (chmod \+x) and begin with a valid shebang, such as \#\!/bin/sh.17

### **2.3 The macOS Code Signing and Notarization Gauntlet: A Step-by-Step Guide**

This multi-stage process is the most complex and failure-prone aspect of macOS distribution. It requires obtaining cryptographic identities from Apple, correctly configuring the application's permissions, signing all executable code, and submitting the application to Apple's automated malware scanner.

#### **Step 2.3.1: Obtaining Apple Developer ID Certificates**

Before any code can be signed, a developer must establish their identity with Apple.

1. **Enroll in the Apple Developer Program:** This is a mandatory first step and requires an annual fee. Enrollment provides access to the necessary certificates, identifiers, and profiles portal.18
2. **Understand Certificate Types:** For distributing an application outside the Mac App Store, two specific certificates are required:
   * **Developer ID Application:** Used to sign the .app bundle and all executable code within it, including frameworks, helper apps, and the system extension.19
   * **Developer ID Installer:** Used exclusively to sign the final .pkg installer package.21
3. **Generate a Certificate Signing Request (CSR):** On a macOS machine, open the **Keychain Access** application. From the menu, select Keychain Access \> Certificate Assistant \> Request a Certificate From a Certificate Authority.... Fill in the required information and choose "Saved to disk" to generate a .certSigningRequest file.23
4. **Create the Certificates:** Log in to the Apple Developer Portal, navigate to "Certificates, Identifiers & Profiles," and create a new certificate. Select the appropriate type ("Developer ID Application" or "Developer ID Installer"), and upload the CSR file generated in the previous step. Repeat the process for both certificate types.24
5. **Install the Certificates:** Download the generated .cer files from the Developer Portal and double-click them. This will install them into the login keychain on the local machine, making them available to code signing tools.24

#### **Step 2.3.2: Configuring Hardened Runtime and Entitlements**

Notarization by Apple requires that the application is built with the Hardened Runtime enabled. This is a security feature that protects the application from certain classes of exploits, such as code injection.

1. **Enable Hardened Runtime:** In electron-builder.yml, add the following key to the mac configuration:
   YAML
   mac:
     hardenedRuntime: true

   25
2. **Create Entitlement Files:** The Hardened Runtime restricts the application's capabilities by default. To allow necessary operations, such as just-in-time (JIT) compilation used by Electron's V8 JavaScript engine, specific permissions, known as entitlements, must be granted. These are defined in .plist (Property List) XML files. Two files are required in the build directory:
   * build/entitlements.mac.plist: This file defines the entitlements for the main application bundle.
   * build/entitlements.mac.inherit.plist: This file is used for all child processes and binaries, ensuring they inherit the necessary permissions.27

The content of these files will be nearly identical and must include standard entitlements for Electron to function correctly, as well as the specific entitlement required to manage system extensions.XML
\<?xml version="1.0" encoding="UTF-8"?\>
\<\!DOCTYPE **plist** **PUBLIC** "-//Apple//DTD PLIST 1.0//EN" "<https://www.apple.com/DTDs/PropertyList-1.0.dtd"\>>>>
\<plist version\="1.0"\>
\<dict\>
    \<key\>com.apple.security.cs.allow-jit\</key\>
    \<true/\>
    \<key\>com.apple.security.cs.allow-unsigned-executable-memory\</key\>
    \<true/\>
    \<key\>com.apple.security.cs.disable-library-validation\</key\>
    \<true/\>
    \<key\>com.apple.developer.system-extension.install\</key\>
    \<true/\>
\</dict\>
\</plist\>
29

3. **Configure electron-builder:** Point electron-builder to these files in the mac configuration:
   YAML
   mac:
     hardenedRuntime: true
     entitlements: build/entitlements.mac.plist
     entitlementsInherit: build/entitlements.mac.inherit.plist

   28

The following table summarizes the essential entitlement keys for an Electron application deploying a system extension.

| Entitlement Key | Purpose | Target |
| :---- | :---- | :---- |
| com.apple.security.cs.allow-jit | Allows the V8 JavaScript engine to generate and execute machine code. | Main App & Inherit |
| com.apple.security.cs.allow-unsigned-executable-memory | Allows writing to executable memory pages, required by V8. | Main App & Inherit |
| com.apple.security.cs.disable-library-validation | Allows the app to load libraries and plugins that are not signed by Apple or with the same Team ID. Required for many native Node.js modules. | Main App & Inherit |
| com.apple.developer.system-extension.install | Grants the application permission to submit activation and deactivation requests for system extensions. | Main App |
| com.apple.developer.endpoint-security.client | *Example:* Grants an Endpoint Security extension permission to monitor system events. | System Extension |

#### **Step 2.3.3: Signing All Binaries, Including the System Extension**

While electron-builder automatically signs the main application executable and its bundled frameworks, it is not aware of arbitrary binaries added via extraFiles. The .systemextension is a separate executable bundle and must be explicitly signed. Failure to do so will result in immediate rejection by the notarization service.

The mac.binaries configuration option is used for this purpose. It takes an array of paths to additional executables that need to be signed. The path should point to the location of the binary *within the packaged application structure*.

YAML

mac:
  \#... other mac configurations
  binaries:
    \- "dist/mac/MyApp.app/Contents/Library/SystemExtensions/MyExtension.systemextension"

This configuration ensures that during the signing phase, electron-builder will locate the system extension bundle and apply the correct Developer ID Application signature and entitlements to it before sealing the main .app bundle.25

#### **Step 2.3.4: Implementing the afterSign Notarization Hook**

Notarization is the process where the signed application is uploaded to Apple's servers for an automated security scan. electron-builder facilitates this through an afterSign hook, which executes a script after the .app bundle is fully packaged and signed, but before the final .pkg installer is created.35

1. **Install @electron/notarize:** Add the notarization tool as a development dependency: npm install @electron/notarize \--save-dev.
2. **Create the notarize.js Script:** Create a script, for example in build/notarize.js, that will perform the notarization. This script uses the @electron/notarize package and relies on environment variables to securely access credentials.
   JavaScript
   // build/notarize.js
   const { notarize } \= require('@electron/notarize');

   exports.default \= async function notarizing(context) {
     const { electronPlatformName, appOutDir } \= context;
     if (electronPlatformName\!== 'darwin') {
       return;
     }

     if (\!process.env.APPLE\_ID ||\!process.env.APPLE\_APP\_SPECIFIC\_PASSWORD ||\!process.env.APPLE\_TEAM\_ID) {
       console.warn('Skipping notarization: Apple credentials not provided in environment variables.');
       return;
     }

     const appName \= context.packager.appInfo.productFilename;
     const appPath \= \`${appOutDir}/${appName}.app\`;
     const appBundleId \= context.packager.appInfo.id;

     console.log(\`Starting notarization for ${appBundleId} at ${appPath}\`);

     try {
       await notarize({
         tool: 'notarytool',
         appBundleId: appBundleId,
         appPath: appPath,
         appleId: process.env.APPLE\_ID,
         appleIdPassword: process.env.APPLE\_APP\_SPECIFIC\_PASSWORD,
         teamId: process.env.APPLE\_TEAM\_ID,
       });
       console.log('Notarization completed successfully.');
     } catch (error) {
       console.error('Notarization failed:', error);
       throw error; // Fail the build
     }
   };

   31
   This script correctly identifies the platform, constructs the path to the signed .app, and calls the notarize function using the modern notarytool method. It requires an APPLE\_ID (your developer account email), an APPLE\_APP\_SPECIFIC\_PASSWORD (which must be generated at appleid.apple.com), and your APPLE\_TEAM\_ID.40
3. **Configure the Hook in electron-builder.yml:** Finally, link this script in the top-level build configuration:
   YAML
   afterSign: build/notarize.js

The decision to include a system extension initiates a cascade of dependencies that fundamentally reshapes the entire macOS release pipeline. It is not an isolated feature but an architectural commitment. This choice immediately invalidates the simpler .dmg distribution method and forces the project down the most rigorous path Apple provides for non-App Store distribution. This path requires a .pkg installer, which in turn demands signing with a distinct Developer ID Installer certificate. The overarching requirement for notarization then pulls in its own dependencies: enabling the Hardened Runtime, which then necessitates the careful crafting of .plist entitlement files to grant the application permissions it lost. The build process must be orchestrated with precision to follow this sequence: first, inject the system extension file; second, package the application; third, sign all executable components including the extension; fourth, notarize the entire signed application bundle; and finally, package and sign the resulting installer. This chain of requirements underscores the significant investment in time and expertise needed for deploying advanced macOS features within an Electron application.

## **Part III: Engineering Windows Installers and Code Signing**

While macOS presents a gauntlet of notarization and entitlements, Windows distribution comes with its own set of challenges, particularly concerning installer standards for enterprise environments and the evolving landscape of code signing security. This section details the process of creating a professional MSI installer, provides a comparative analysis of the more flexible NSIS alternative, and offers a comprehensive guide to the modern Authenticode code signing process, with a crucial focus on the industry-wide shift to hardware-based security tokens.

### **3.1 Building the MSI Installer**

The Microsoft Installer (MSI) format is the standard for software installation, updates, and removal in Windows environments, especially within corporate settings where deployments are often managed via Group Policy. electron-builder provides first-class support for generating MSI packages through its msi target.

The configuration for the MSI installer resides within the win block of electron-builder.yml, under an msi key.42 Key options include:

* oneClick: A boolean that determines the installer's user interface. When true (the default), it creates a streamlined, one-click installation process. When false, it provides a more traditional, multi-page "assisted" installer wizard.42
* perMachine: A boolean that controls the installation scope. If false (the default), the application is installed only for the current user. If true, the installer will request administrator privileges to install the application for all users on the system.42
* createDesktopShortcut and createStartMenuShortcut: Booleans that control the creation of common application shortcuts.42
* upgradeCode: A specific GUID (Globally Unique Identifier) that identifies the application family. It is crucial to set this value and keep it consistent across all versions of the application. The upgradeCode allows the Windows Installer service to correctly handle upgrades, ensuring that installing a new version properly replaces the old one.42 If not specified, electron-builder generates one based on the appId, but explicitly defining it is a best practice.

An example configuration for an MSI target might look like this:

YAML

win:
  target:
    \- target: msi
      arch:
        \- x64
  msi:
    oneClick: false
    perMachine: true
    createDesktopShortcut: true
    createStartMenuShortcut: true
    upgradeCode: 'YOUR-GUID-HERE' \# Generate a GUID once and keep it

### **3.2 The NSIS Alternative: A Comparative Overview**

While MSI is the enterprise standard, electron-builder's default and often more powerful installer technology for Windows is NSIS (Nullsoft Scriptable Install System). NSIS offers greater flexibility and a richer set of features for creating a polished user installation experience.43

NSIS provides several key advantages:

* **Advanced Customization:** NSIS is fully scriptable. electron-builder allows for extensive customization by providing a custom installer.nsh script. This enables developers to add custom pages to the installer, execute specific commands during installation or uninstallation, or modify system settings.43
* **Web Installers:** electron-builder supports an nsis-web target. This creates a small initial installer executable that, when run, downloads the main application package from a web server. This results in a much smaller initial download for the user and is ideal for applications distributed over the internet.43
* **Unified Multi-Arch Installer:** When building for both 32-bit (ia32) and 64-bit (x64) architectures, NSIS can produce a single installer executable that automatically detects the user's operating system and installs the correct version.43

The choice between MSI and NSIS depends on the target audience. For applications intended for deployment in managed corporate environments, MSI is often the mandatory choice. For consumer-facing applications distributed directly to users, NSIS generally provides a superior and more customizable installation experience.

### **3.3 The Windows Authenticode Signing Process**

Code signing on Windows is essential. Unsigned applications will trigger the intimidating SmartScreen filter, which strongly discourages users from running the software and can severely harm user trust and adoption rates. The process involves obtaining a special Authenticode certificate and using it to apply a digital signature to the application's installer and executables.

#### **Step 3.3.1: Acquiring an Authenticode Certificate**

Authenticode certificates must be purchased from a trusted Certificate Authority (CA). There are two primary types:

* **Organization Validation (OV):** This certificate requires the CA to perform a basic vetting of the organization's identity. It will prevent the most severe SmartScreen warnings, but a new application may still show a warning until it builds a reputation with Microsoft's services.46
* **Extended Validation (EV):** This certificate involves a much more rigorous and lengthy validation process by the CA. In return, applications signed with an EV certificate are immediately trusted by Microsoft SmartScreen, eliminating the "reputation-building" period. EV certificates are more expensive and have stricter handling requirements.46

Leading CAs for Authenticode certificates include DigiCert and Sectigo. The purchasing process typically involves selecting the certificate type, providing organizational details for validation, and completing a payment.47

#### **Step 3.3.2: Signing with Hardware Tokens (The Modern Standard)**

The landscape of code signing has undergone a significant security-driven transformation. In response to widespread private key compromises, the CA/Browser Forum—the industry body that governs certificate issuance—now mandates that all new OV and EV code signing certificates issued after June 1, 2023, must have their private keys generated and stored on a FIPS 140-2 Level 2 compliant hardware security module. For most developers, this means the certificate is delivered on a physical USB token.47

This shift renders older, file-based signing workflows using .pfx files obsolete for any newly issued certificates. The signing process now directly involves interacting with this hardware.

Local Development Signing Workflow:
For signing on a local development machine, the process is as follows:

1. **Install Hardware Token Drivers:** The USB token requires specific drivers and client software to be installed on the Windows machine. For example, tokens from many CAs use the SafeNet Authentication Client software. This software allows the Windows Certificate Store to see and use the certificate stored on the token.50
2. **Install the Windows SDK:** electron-builder uses Microsoft's signtool.exe utility to perform the signing. This tool is part of the Windows SDK, which must be installed on the build machine.51
3. **Configure electron-builder:** With the hardware token plugged in and its drivers installed, the code signing certificate will appear in the Windows Certificate Store. Instead of using the certificateFile and certificatePassword options in electron-builder.yml, the configuration must now identify the certificate by its subject name or SHA1 thumbprint.
   YAML
   win:
     certificateSubjectName: "Your Company Name, Inc."

   50
4. **Execute the Build:** When electron-builder runs, it will invoke signtool.exe. signtool.exe will find the specified certificate in the certificate store and communicate with the hardware token. The token's client software will then display a dialog box prompting the user to enter the token's PIN. Upon successful PIN entry, the hardware token performs the cryptographic signing operation and returns the signature to signtool.exe, which embeds it into the application file.

This workflow, while secure, presents a significant challenge for automation. The requirement for a physically present USB token and a manual PIN entry is fundamentally incompatible with ephemeral, non-interactive CI/CD environments like GitHub-hosted runners. This has effectively bifurcated the Windows signing process into a local, manual workflow and a separate, automated workflow.

For automated CI/CD pipelines, the solution is to move the signing key and operation to the cloud. Services like Azure Key Vault with its Trusted Signing feature, or other cloud-based Hardware Security Modules (HSMs), allow for the secure storage of private keys and provide an API for signing operations. This requires a different approach:

1. **Provision a Cloud HSM:** The organization must set up a service like Azure Key Vault and securely import or generate the code signing certificate within it.53
2. **Use a Specialized Signing Tool:** Tools like Microsoft's AzureSignTool are designed to authenticate with the cloud HSM (e.g., via service principals or managed identities) and use it to sign files remotely.53
3. **Integrate with a Custom Signing Script:** electron-builder accommodates this workflow via the win.sign configuration option. This option allows specifying a path to a custom JavaScript signing script.54 This script receives the path to the file that needs to be signed and can then execute the necessary commands to invoke AzureSignTool or a similar utility, passing the required authentication details (securely, via environment variables).

This modern reality means that a scalable, automated release pipeline for Windows applications must now be architected as a cloud-native process. The initial decision to implement automated builds directly influences the choice of certificate storage, forcing a move away from simple USB tokens to more complex but automatable cloud HSM solutions.

## **Part IV: Streamlining Linux Distribution with AppImage**

The Linux desktop environment is characterized by its diversity, with numerous distributions (like Ubuntu, Fedora, and Arch Linux) each having its own preferred package management system (.deb, .rpm, etc.). To provide a universal solution that works across this fragmented landscape, the AppImage format has emerged as a leading choice for Electron application developers. It offers a self-contained, distribution-agnostic package that requires no installation or special permissions to run.

### **4.1 Configuring the AppImage Target**

electron-builder provides excellent, out-of-the-box support for creating AppImage packages. The configuration is straightforward and is handled within the linux block of the electron-builder.yml file.

To enable AppImage as a build target, it should be added to the target array within the linux configuration. Further customization can be provided in a dedicated appImage block.3

YAML

linux:
  target:
    \- AppImage
  category: Utility \# Specifies the application category for desktop menus
  appImage:
    license: license.txt \# Path to the license file to be included
    desktop:
      Name: MyApp
      Comment: An amazing cross-platform application.
      Icon: build/icon.png

Key configuration options include:

* target: An array of desired output formats. For this use case, it is set to \['AppImage'\]. Other options like deb or rpm could be included to generate distribution-specific packages as well.3
* category: Specifies the application category according to the FreeDesktop.org standard, which helps desktop environments organize the application in their menus (e.g., Utility, Development, Game).57
* appImage: A dedicated object for AppImage-specific settings.57
  * license: The path to a license file (e.g., LICENSE.txt) that will be embedded within the AppImage.
  * desktop: An object that allows for the customization of the .desktop file, which is used by desktop environments to create menu entries and shortcuts. This includes keys like Name, Comment, and Icon.

### **4.2 Desktop Integration and Best Practices**

The primary advantage of the AppImage format is its simplicity and portability. An AppImage is a single, executable file that contains the application and all its dependencies. Users can download this file, make it executable (chmod \+x myapp.AppImage), and run it immediately, without any installation process or need for administrator privileges.58 This "one file, one app" model significantly simplifies distribution for developers and provides a consistent experience for users across different Linux distributions.

Historically, one drawback of AppImages was the lack of automatic desktop integration (i.e., adding the application to the system's application menu). However, this is no longer a direct concern for the packager. Since electron-builder version 21, the responsibility of desktop integration has shifted from the AppImage itself to optional, user-installed helper tools.57 The most prominent of these is AppImageLauncher, a utility that, when installed, monitors for new AppImage files and offers to automatically move them to a central location and create the necessary menu entries.57 This approach keeps the AppImage file itself clean and portable while providing a seamless integration experience for users who opt into it.

While electron-builder also supports creating .deb packages for Debian/Ubuntu-based systems and .rpm packages for Fedora/RedHat-based systems, these formats introduce additional complexity.3 To provide automatic updates for .deb and .rpm packages, a developer must set up and maintain their own APT or YUM package repositories, which is a significant operational overhead.58 The AppImage format, in contrast, works seamlessly with electron-builder's built-in auto-update mechanism (electron-updater), making it the most efficient and highly recommended starting point for any Electron application targeting the Linux desktop.58

## **Part V: Automating the Cross-Platform Build and Release Pipeline**

The final and most critical phase of establishing a professional distribution workflow is automation. A robust Continuous Integration and Continuous Deployment (CI/CD) pipeline eliminates manual, error-prone release steps, ensures consistency across builds, and enables rapid delivery of new versions to users. This section details how to integrate the previously defined configurations for macOS, Windows, and Linux into a single, automated build and release workflow using GitHub Actions.

### **5.1 A CI/CD Workflow with GitHub Actions**

GitHub Actions provides a powerful and convenient platform for automating software workflows directly within a project's repository. It allows for the creation of jobs that run on virtual machines for all three major operating systems, making it an ideal choice for cross-platform Electron builds.

The core of the automation is a workflow file, typically located at .github/workflows/release.yml. This YAML file defines the triggers, jobs, and steps for the entire process. A key feature for cross-platform builds is the strategy.matrix, which allows a single job definition to be run in parallel across multiple operating systems.59

It is essential to respect platform constraints within the workflow. macOS applications can only be properly code-signed and notarized on a macOS runner, as the process relies on Keychain Access and other macOS-specific tools.60 Similarly, while it is technically possible to build some Windows targets on Linux using Wine, creating and signing installers is most reliably done on a native Windows runner.60

A sample workflow file that builds all three targets in parallel would be structured as follows:

YAML

name: Build and Release

on:
  push:
    tags:
      \- 'v\*.\*.\*' \# Trigger workflow on tag pushes like v1.0.0

jobs:
  release:
    runs-on: ${{ matrix.os }}

    strategy:
      matrix:
        os: \[macos-latest, windows-latest, ubuntu-latest\]

    steps:
      \- name: Check out Git repository
        uses: actions/checkout@v3

      \- name: Install Node.js
        uses: actions/setup-node@v3
        with:
          node-version: 18

      \- name: Install dependencies
        run: npm install

      \- name: Build and release Electron app
        env:
          \# \--- macOS Credentials \---
          CSC\_LINK: ${{ secrets.MAC\_CERTS\_P12\_BASE64 }}
          CSC\_KEY\_PASSWORD: ${{ secrets.MAC\_CERTS\_PASSWORD }}
          APPLE\_ID: ${{ secrets.APPLE\_ID }}
          APPLE\_APP\_SPECIFIC\_PASSWORD: ${{ secrets.APPLE\_APP\_SPECIFIC\_PASSWORD }}
          APPLE\_TEAM\_ID: ${{ secrets.APPLE\_TEAM\_ID }}

          \# \--- Windows Credentials \---
          \# For hardware token signing, these would be used by a custom sign script
          \# Example for Azure Key Vault:
          AZURE\_TENANT\_ID: ${{ secrets.AZURE\_TENANT\_ID }}
          AZURE\_CLIENT\_ID: ${{ secrets.AZURE\_CLIENT\_ID }}
          AZURE\_CLIENT\_SECRET: ${{ secrets.AZURE\_CLIENT\_SECRET }}

          \# \--- GitHub Token for Publishing \---
          GITHUB\_TOKEN: ${{ secrets.GITHUB\_TOKEN }}
        run: npm run dist \-- \--publish always

59

### **5.2 Secure Credential Management with GitHub Secrets**

The code signing and notarization processes for macOS and Windows require a multitude of sensitive credentials, including certificate files, private key passwords, and API keys. These secrets must **never** be committed directly into the source code repository.

GitHub Actions provides a secure solution for this problem through **Repository Secrets**. These are encrypted environment variables that can be created in the repository's settings (Settings \> Secrets and variables \> Actions) and are only exposed to the workflow runner during execution.59

The workflow involves:

1. **Preparing the Secrets:**
   * For macOS, the .p12 file containing the Developer ID Application and Installer certificates and their private keys must be Base64 encoded into a single string. On macOS, this can be done with the command: base64 \-i certs.p12 \-o encoded.txt. The contents of encoded.txt become a secret.19
   * Passwords, API keys, and other string-based credentials can be copied directly.
2. **Creating the Secrets in GitHub:** For each piece of sensitive information, a new repository secret is created. The names should be descriptive (e.g., MAC\_CERTS\_P12\_BASE64, APPLE\_APP\_SPECIFIC\_PASSWORD).
3. **Mapping Secrets to Environment Variables:** In the GitHub Actions workflow file, the env block is used to map these secrets to the specific environment variable names that electron-builder expects. For example, the MAC\_CERTS\_P12\_BASE64 secret is mapped to the CSC\_LINK environment variable.19 This mapping is the critical link that securely provides the necessary credentials to the build tool at runtime.

The following table serves as a practical checklist for the essential environment variables required for a fully automated, cross-platform signing process in a CI/CD environment.

| Purpose | Environment Variable | GitHub Secret Example | Notes |
| :---- | :---- | :---- | :---- |
| macOS Certificate File | CSC\_LINK | MAC\_CERTS\_P12\_BASE64 | The Base64-encoded content of the .p12 file containing both Developer ID Application and Installer certificates. |
| macOS Certificate Password | CSC\_KEY\_PASSWORD | MAC\_CERTS\_PASSWORD | The password used to encrypt the .p12 file. |
| macOS Notarization (Apple ID) | APPLE\_ID | APPLE\_ID | The email address of the Apple Developer account. |
| macOS Notarization (Password) | APPLE\_APP\_SPECIFIC\_PASSWORD | APPLE\_APP\_SPECIFIC\_PASSWORD | An app-specific password generated from the Apple ID account page. |
| macOS Notarization (Team ID) | APPLE\_TEAM\_ID | APPLE\_TEAM\_ID | The Team ID from the Apple Developer Portal. |
| Windows Signing (Cloud HSM) | *Custom* | AZURE\_CLIENT\_ID, etc. | These variables are not directly used by electron-builder but are passed to a custom signing script (win.sign) that interacts with a cloud HSM service like Azure Key Vault. |

### **5.3 Publishing Releases to GitHub**

The final step of the automated pipeline is to publish the generated installers as a new release. electron-builder has built-in support for publishing artifacts to various providers, including GitHub Releases.62

1. **Triggering the Release:** The workflow example is configured to trigger only when a new Git tag matching the pattern v\*.\*.\* is pushed to the repository. This is a common and effective release strategy: development happens on branches, and a tag is created only when a new version is ready for release.59
2. **Configuring the publish Provider:** In electron-builder.yml, the publish property should be configured to use the github provider. Often, this can be auto-detected from the repository's package.json or Git configuration, but explicit configuration is a good practice.62
   YAML
   publish:
     provider: github

3. **Executing the Publish Command:** The build command in the workflow file includes the \--publish always flag (npm run dist \-- \--publish always). This tells electron-builder that, after a successful build, it should proceed with the publishing step.
4. **Automatic Release Creation:** When running in a GitHub Actions environment, electron-builder automatically uses the GITHUB\_TOKEN secret, which is provided by the runner and has permissions to interact with the repository. It will:
   * Look for a draft release corresponding to the Git tag. If one exists, it will upload the artifacts to it. If not, it will create a new draft release.
   * Upload all generated installer files (.pkg, .msi, .AppImage) to the release.
   * Generate and upload platform-specific update manifest files (e.g., latest-mac.yml, latest.yml), which are essential for the electron-updater module to automatically detect and download new versions.62

By combining a matrix build strategy, secure credential management with GitHub Secrets, and electron-builder's integrated publishing capabilities, this workflow provides a fully automated, "push-to-release" pipeline for a complex, cross-platform Electron application.

## **Conclusion**

The journey from a functional Electron application to a professionally packaged, signed, and distributed product is a complex undertaking that extends far beyond simple code compilation. The analysis reveals that modern application distribution is fundamentally a discipline of security, compliance, and platform-specific engineering. A successful strategy requires a deep understanding of the distinct and increasingly stringent requirements imposed by each major operating system.

For macOS, the decision to incorporate advanced functionality like a System Extension serves as a critical architectural inflection point. This single choice triggers a cascade of non-negotiable requirements, moving the entire distribution model from a simple disk image to a complex installer package, and mandating a rigorous process of obtaining developer certificates, enabling the Hardened Runtime, crafting precise entitlements, and integrating with Apple's notarization service. The process is a testament to Apple's layered security model, where each capability is gated by a corresponding set of cryptographic and procedural checks.

For Windows, the industry-wide shift to hardware-bound code signing certificates has bifurcated the development and deployment workflow. While local signing with a physical USB token remains viable for manual builds, it presents a fundamental barrier to automation. The forward-looking solution lies in embracing cloud-native security paradigms, utilizing cloud-based Hardware Security Modules and custom signing scripts to integrate with CI/CD pipelines. This represents a significant shift in both cost and complexity but is now the standard for secure, automated Windows software delivery.

In contrast, the Linux ecosystem, often characterized by its fragmentation, finds a unifying solution in the AppImage format. Its self-contained, distribution-agnostic nature, combined with seamless support from electron-builder and its auto-update mechanisms, makes it the most efficient path for reaching the broad spectrum of Linux users.

Ultimately, managing this multifaceted complexity at scale is only feasible through a unified configuration and a robust automation pipeline. The use of a centralized electron-builder.yml file acts as a single source of truth, taming the divergent requirements of each platform within a coherent structure. Layering this with a powerful CI/CD system like GitHub Actions, which provides secure credential management and parallel builds across all target operating systems, transforms the release process from a manual, error-prone chore into a reliable, repeatable, and automated workflow. The investment in architecting this pipeline is substantial, but it is an essential prerequisite for any serious cross-platform desktop application in today's security-conscious landscape.

### **Works cited**

1. Electron Forge: Getting Started, accessed October 14, 2025, [https://www.electronforge.io/](https://www.electronforge.io/)
2. Packaging Your Application | Electron, accessed October 14, 2025, [https://electronjs.org/docs/latest/tutorial/tutorial-packaging](https://electronjs.org/docs/latest/tutorial/tutorial-packaging)
3. electron-builder, accessed October 14, 2025, [https://www.electron.build/](https://www.electron.build/)
4. Building your First App | Electron, accessed October 14, 2025, [https://electronjs.org/docs/latest/tutorial/tutorial-first-app](https://electronjs.org/docs/latest/tutorial/tutorial-first-app)
5. Building an Electron app from scratch (Part 1\) | by Mark Jordan | Ingeniously Simple, accessed October 14, 2025, [https://medium.com/ingeniouslysimple/building-an-electron-app-from-scratch-part-1-a1d9012c146a](https://medium.com/ingeniouslysimple/building-an-electron-app-from-scratch-part-1-a1d9012c146a)
6. Common Configuration \- electron-builder, accessed October 14, 2025, [https://www.electron.build/configuration.html](https://www.electron.build/configuration.html)
7. Electron Builder \- Visual Studio Marketplace, accessed October 14, 2025, [https://marketplace.visualstudio.com/items?itemName=idleberg.electron-builder](https://marketplace.visualstudio.com/items?itemName=idleberg.electron-builder)
8. electron builder.Interface.Configuration, accessed October 14, 2025, [https://www.electron.build/electron-builder.interface.configuration](https://www.electron.build/electron-builder.interface.configuration)
9. Application Contents \- electron-builder, accessed October 14, 2025, [https://www.electron.build/contents.html](https://www.electron.build/contents.html)
10. Electron-Builder include external folder \- Stack Overflow, accessed October 14, 2025, [https://stackoverflow.com/questions/61599298/electron-builder-include-external-folder](https://stackoverflow.com/questions/61599298/electron-builder-include-external-folder)
11. Packaging different binaries per platform for Electron \- Stack Overflow, accessed October 14, 2025, [https://stackoverflow.com/questions/62829863/packaging-different-binaries-per-platform-for-electron](https://stackoverflow.com/questions/62829863/packaging-different-binaries-per-platform-for-electron)
12. System Extensions | Apple Developer Documentation, accessed October 14, 2025, [https://developer.apple.com/documentation/systemextensions](https://developer.apple.com/documentation/systemextensions)
13. How to embed a mac app extension in an Electron app? \- Stack Overflow, accessed October 14, 2025, [https://stackoverflow.com/questions/45612515/how-to-embed-a-mac-app-extension-in-an-electron-app](https://stackoverflow.com/questions/45612515/how-to-embed-a-mac-app-extension-in-an-electron-app)
14. System Extensions \- Overview and Guide \- Kandji Support, accessed October 14, 2025, [https://support.kandji.io/kb/system-extensions-overview-and-guide](https://support.kandji.io/kb/system-extensions-overview-and-guide)
15. System Extensions and DriverKit \- Apple Developer, accessed October 14, 2025, [https://developer.apple.com/system-extensions/](https://developer.apple.com/system-extensions/)
16. Notarizing macOS software before distribution | Apple Developer Documentation, accessed October 14, 2025, [https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
17. macOS Kernel Extensions \- electron-builder, accessed October 14, 2025, [https://www.electron.build/tutorials/macos-kernel-extensions.html](https://www.electron.build/tutorials/macos-kernel-extensions.html)
18. Code Signing | Electron, accessed October 14, 2025, [https://electronjs.org/docs/latest/tutorial/code-signing](https://electronjs.org/docs/latest/tutorial/code-signing)
19. How to code-sign and notarize an Electron application for macOS \- BigBinary, accessed October 14, 2025, [https://www.bigbinary.com/blog/code-sign-notorize-mac-desktop-app](https://www.bigbinary.com/blog/code-sign-notorize-mac-desktop-app)
20. Getting code signing certificates | Signing Your Applications and Building Installers | Tutorials & Manuals, accessed October 14, 2025, [https://revolution.screenstepslive.com/s/revolution/m/10695/l/112989-getting-code-signing-certificates](https://revolution.screenstepslive.com/s/revolution/m/10695/l/112989-getting-code-signing-certificates)
21. MacOS \- electron-builder, accessed October 14, 2025, [https://www.electron.build/code-signing-mac.html](https://www.electron.build/code-signing-mac.html)
22. Cannot find valid code signing certificate despite valid identities being printed in console during build \#2513 \- GitHub, accessed October 14, 2025, [https://github.com/electron-userland/electron-builder/issues/2513](https://github.com/electron-userland/electron-builder/issues/2513)
23. How to get a certificate, the process of code-signing & notarization of macOS binaries for distribution outside of the Apple App Store. \- Software, accessed October 14, 2025, [https://dennisbabkin.com/blog/?t=how-to-get-certificate-code-sign-notarize-macos-binaries-outside-apple-app-store](https://dennisbabkin.com/blog/?t=how-to-get-certificate-code-sign-notarize-macos-binaries-outside-apple-app-store)
24. Create Developer ID certificates \- Certificates \- Account \- Help ..., accessed October 14, 2025, [https://developer.apple.com/help/account/certificates/create-developer-id-certificates/](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/)
25. Any macOS Target \- electron-builder, accessed October 14, 2025, [https://www.electron.build/mac.html](https://www.electron.build/mac.html)
26. Notarizing your Electron application | Kilian Valkhof, accessed October 14, 2025, [https://kilianvalkhof.com/2019/electron/notarizing-your-electron-application/](https://kilianvalkhof.com/2019/electron/notarizing-your-electron-application/)
27. 3\. App Sandbox and Entitlements · electron/osx-sign Wiki \- GitHub, accessed October 14, 2025, [https://github.com/electron/osx-sign/wiki/3.-App-Sandbox-and-Entitlements](https://github.com/electron/osx-sign/wiki/3.-App-Sandbox-and-Entitlements)
28. electron builder.Interface.MacConfiguration, accessed October 14, 2025, [https://www.electron.build/electron-builder.interface.macconfiguration](https://www.electron.build/electron-builder.interface.macconfiguration)
29. System Extensions | Apple Developer Documentation, accessed October 14, 2025, [https://developer.apple.com/documentation/bundleresources/system-extensions](https://developer.apple.com/documentation/bundleresources/system-extensions)
30. macOS System Extension: Entitlements and Signing with Provisioning Profile, accessed October 14, 2025, [https://stackoverflow.com/questions/63927475/macos-system-extension-entitlements-and-signing-with-provisioning-profile](https://stackoverflow.com/questions/63927475/macos-system-extension-entitlements-and-signing-with-provisioning-profile)
31. Making notarization work on macOS for Electron apps built with ..., accessed October 14, 2025, [https://christarnowski.com/making-notarization-work-on-macos-for-electron-apps-built-with-electron-builder/](https://christarnowski.com/making-notarization-work-on-macos-for-electron-apps-built-with-electron-builder/)
32. Releasing an Electron app on the Mac App Store | by Evan Conrad | Medium, accessed October 14, 2025, [https://medium.com/@flaqueEau/releasing-an-electron-app-on-the-mac-app-store-c32dfcd9c2bd](https://medium.com/@flaqueEau/releasing-an-electron-app-on-the-mac-app-store-c32dfcd9c2bd)
33. MAS \- electron-builder, accessed October 14, 2025, [https://www.electron.build/mas.html](https://www.electron.build/mas.html)
34. Sign electron app with packaged prebuilt binaries \[closed\] \- Stack Overflow, accessed October 14, 2025, [https://stackoverflow.com/questions/79755524/sign-electron-app-with-packaged-prebuilt-binaries](https://stackoverflow.com/questions/79755524/sign-electron-app-with-packaged-prebuilt-binaries)
35. Build Hooks \- electron-builder, accessed October 14, 2025, [https://www.electron.build/hooks.html](https://www.electron.build/hooks.html)
36. How I sign and notarize my Electron app on MacOS \- Ayron's Blog, accessed October 14, 2025, [https://www.funtoimagine.com/blog/electron-mac-sign-and-notarize/](https://www.funtoimagine.com/blog/electron-mac-sign-and-notarize/)
37. Need help: code signing mac electron app. : r/electronjs \- Reddit, accessed October 14, 2025, [https://www.reddit.com/r/electronjs/comments/1exwnkm/need\_help\_code\_signing\_mac\_electron\_app/](https://www.reddit.com/r/electronjs/comments/1exwnkm/need_help_code_signing_mac_electron_app/)
38. Top 10 Examples of electron-notarize code in Javascript \- CloudDefense.AI, accessed October 14, 2025, [https://www.clouddefense.ai/code/javascript/example/electron-notarize](https://www.clouddefense.ai/code/javascript/example/electron-notarize)
39. Notarizing my macOS Electron app using vite-electron-builder | YY-EN40P BLOG, accessed October 14, 2025, [https://yy-en40p.com/blog/notarizing-my-macos-electron-app-using-vite-electron-builder/](https://yy-en40p.com/blog/notarizing-my-macos-electron-app-using-vite-electron-builder/)
40. omkarcloud/macos-code-signing-example: Learn how to sign and notarize an Electron app for Mac OS and automate the process using GitHub Actions., accessed October 14, 2025, [https://github.com/omkarcloud/macos-code-signing-example](https://github.com/omkarcloud/macos-code-signing-example)
41. Signing a macOS app \- Electron Forge, accessed October 14, 2025, [https://www.electronforge.io/guides/code-signing/code-signing-macos](https://www.electronforge.io/guides/code-signing/code-signing-macos)
42. MSI \- electron-builder, accessed October 14, 2025, [https://www.electron.build/msi.html](https://www.electron.build/msi.html)
43. NSIS \- electron-builder, accessed October 14, 2025, [https://www.electron.build/nsis.html](https://www.electron.build/nsis.html)
44. Any Windows Target \- electron-builder, accessed October 14, 2025, [https://www.electron.build/win.html](https://www.electron.build/win.html)
45. electron builder.Interface.NsisOptions, accessed October 14, 2025, [https://www.electron.build/electron-builder.interface.nsisoptions](https://www.electron.build/electron-builder.interface.nsisoptions)
46. Can I Get a High-Level Explanation of the Electron-Builder Code Signing Process?, accessed October 14, 2025, [https://stackoverflow.com/questions/79142707/can-i-get-a-high-level-explanation-of-the-electron-builder-code-signing-process](https://stackoverflow.com/questions/79142707/can-i-get-a-high-level-explanation-of-the-electron-builder-code-signing-process)
47. Signing a Windows app | Electron Forge, accessed October 14, 2025, [https://www.electronforge.io/guides/code-signing/code-signing-windows](https://www.electronforge.io/guides/code-signing/code-signing-windows)
48. How do you Purchase a Code Signing Certificate? | DigiCert FAQ, accessed October 14, 2025, [https://www.digicert.com/faq/code-signing-trust/how-to-purchase-a-code-signing-certificate](https://www.digicert.com/faq/code-signing-trust/how-to-purchase-a-code-signing-certificate)
49. Buy Code Signing Certificates \- EV & OV Options | Sectigo® Official, accessed October 14, 2025, [https://www.sectigo.com/ssl-certificates-tls/code-signing](https://www.sectigo.com/ssl-certificates-tls/code-signing)
50. How to Sign a Windows App in Electron Builder \- Code Signing Store, accessed October 14, 2025, [https://codesigningstore.com/how-to-sign-a-windows-app-in-electron-builder](https://codesigningstore.com/how-to-sign-a-windows-app-in-electron-builder)
51. EV Authenticode® Program Signing & Timestamping Using SignTool, accessed October 14, 2025, [https://knowledge.digicert.com/tutorials/ev-authenticode-using-signtool](https://knowledge.digicert.com/tutorials/ev-authenticode-using-signtool)
52. electron/windows-sign: Codesign Electron apps for Windows \- GitHub, accessed October 14, 2025, [https://github.com/electron/windows-sign](https://github.com/electron/windows-sign)
53. Signing electron app for windows with an EV certificate in CI : r/electronjs \- Reddit, accessed October 14, 2025, [https://www.reddit.com/r/electronjs/comments/16sgb3u/signing\_electron\_app\_for\_windows\_with\_an\_ev/](https://www.reddit.com/r/electronjs/comments/16sgb3u/signing_electron_app_for_windows_with_an_ev/)
54. Custom sign is called but all files are signed anyway · Issue \#8884 · electron-userland/electron-builder \- GitHub, accessed October 14, 2025, [https://github.com/electron-userland/electron-builder/issues/8884](https://github.com/electron-userland/electron-builder/issues/8884)
55. Sign executables with Electron builder using KSP library \- DigiCert documentation, accessed October 14, 2025, [https://docs.digicert.com/en/digicert-keylocker/code-signing/sign-with-third-party-signing-tools/windows-applications/sign-executables-with-electron-builder-using-ksp-library.html](https://docs.digicert.com/en/digicert-keylocker/code-signing/sign-with-third-party-signing-tools/windows-applications/sign-executables-with-electron-builder-using-ksp-library.html)
56. How to programmatically sign .exe with Electron-Builder? \- Stack Overflow, accessed October 14, 2025, [https://stackoverflow.com/questions/78541645/how-to-programmatically-sign-exe-with-electron-builder](https://stackoverflow.com/questions/78541645/how-to-programmatically-sign-exe-with-electron-builder)
57. AppImage \- electron-builder, accessed October 14, 2025, [https://www.electron.build/appimage.html](https://www.electron.build/appimage.html)
58. Guide to Distributing Electron Apps For Linux \- Beekeeper Studio, accessed October 14, 2025, [https://www.beekeeperstudio.io/blog/distribute-electron-apps-for-linux](https://www.beekeeperstudio.io/blog/distribute-electron-apps-for-linux)
59. Electron Builder Action \- GitHub Marketplace, accessed October 14, 2025, [https://github.com/marketplace/actions/electron-builder-action](https://github.com/marketplace/actions/electron-builder-action)
60. Multi Platform Build \- electron-builder, accessed October 14, 2025, [https://www.electron.build/multi-platform-build.html](https://www.electron.build/multi-platform-build.html)
61. Setup \- electron-builder, accessed October 14, 2025, [https://www.electron.build/code-signing.html](https://www.electron.build/code-signing.html)
62. Publish \- electron-builder, accessed October 14, 2025, [https://www.electron.build/publish.html](https://www.electron.build/publish.html)
63. Generate the corresponding latest.yml file according to the architecture \#6372 \- GitHub, accessed October 14, 2025, [https://github.com/electron-userland/electron-builder/issues/6372](https://github.com/electron-userland/electron-builder/issues/6372)

