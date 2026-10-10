# Installing Kanavos

Kanavos runs on Windows 10 or 11 and on Linux (x86-64). It is a portable program: unpack it and run it; there is
no installer. (Ελληνικά: [egkatastasi.el.md](egkatastasi.el.md).)

## Windows

1. Open the [latest release](https://github.com/ppapadeas/kanavos/releases/latest) and download
   `kanavos-<version>-windows-x86_64.zip`.
2. Right-click the file › **Extract All…** and choose a folder that belongs to you, for example
   `C:\Users\<you>\AppData\Local\Programs\Kanavos` or a folder in Documents. Avoid `C:\Program Files`: Kanavos
   cannot update itself there.
3. In the unpacked folder, run `kanavos.exe`. Right-click it › **Show more options** › **Send to** › **Desktop
   (create shortcut)** for a desktop icon.
4. The packages are not yet code-signed, so on first launch Windows may show "Windows protected your PC". Click
   **More info**, check that the file is `kanavos.exe`, then **Run anyway**. Windows asks only once per version.

## Linux

1. Download `kanavos-<version>-linux-x86_64.tar.gz` from the
   [latest release](https://github.com/ppapadeas/kanavos/releases/latest). It needs glibc 2.35 or newer (Ubuntu
   22.04, Debian 12, Fedora 36 and later).
2. Unpack it to a folder in your home directory and run it:

   ```sh
   mkdir -p ~/.local/opt
   tar -xzf kanavos-*-linux-x86_64.tar.gz -C ~/.local/opt
   ~/.local/opt/kanavos-*-linux-x86_64/kanavos
   ```

3. Kanavos draws with the graphics card through Vulkan or OpenGL, so the system needs the usual Mesa or vendor
   drivers. File dialogs use the desktop portal (`xdg-desktop-portal`, present on GNOME, KDE and most desktops).
   The interface needs a font with Greek letters (Noto Sans, DejaVu Sans or Liberation Sans); set
   `KANAVOS_FONT=/path/to/font.ttf` to choose another.

## First steps

The first launch opens a sample two-storey house. The [getting-started guide](getting-started.md) (also in the
`docs/guide` folder of the package) walks from the plot to the permit drawings.

## Updates

Kanavos asks GitHub for a newer version at start; the request carries only the version, the operating system and
the architecture, never project data. When one exists, a notice appears at the right of the status bar and in the
**Help** menu. **Install and restart** downloads it, checks its digital signature, asks about unsaved changes and
starts again with your project open. **Later** and **Skip this version** leave things as they are.

- **Help › Check for updates…** checks now.
- **Help › Update channel**: **Stable releases**, or **Test builds**, which also offers pre-releases with the
  newest changes. Testers choose Test builds.
- **Help › Check for updates at start** turns the automatic check off; `KANAVOS_NO_UPDATE_CHECK=1` turns every
  check off.

Kanavos updates itself only in a folder your account can write to. Otherwise download the new archive and unpack
it over the old folder. Details: [updates.md](../updates.md).

## Reporting problems

Open an issue on [GitHub](https://github.com/ppapadeas/kanavos/issues) with what you did, what you expected and
what happened. **Help › About Kanavos** shows the version. Attach the `.kanavos` file only if you are happy to
share the project; a screenshot often says enough.
