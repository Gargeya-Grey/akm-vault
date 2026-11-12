# Getting started with AKM

AKM keeps your API keys in one encrypted file on your own computer. You need no account and no command line.

**This is an early build. Use fake keys only.** It has not passed the independent security review and the testing that must come before real keys. The [README](../README.md) gives the current status.

## What you need

- A Windows 10 or Windows 11 computer.
- The setup program, `AKM_0.1.0_x64-setup.exe`. There is no public download yet.
- An internet connection during setup, if your computer does not have Microsoft WebView2. Most Windows 11 computers have it. AKM's own code has no network client ([README](../README.md#limits)).

## Install

1. Double-click `AKM_0.1.0_x64-setup.exe`.
2. Windows can show **Windows protected your PC**, because this build has no publisher signature. Select **More info**, then **Run anyway**, only if you trust where the file came from.
3. Follow the setup steps. Setup installs AKM for your Windows account only, so it does not ask for administrator approval.
4. Open **AKM** from the Start menu.

## Create your vault

The first time you open AKM, it shows **Start a vault**.

1. **Vault file** shows where AKM will keep your keys. Select **Browse…** to choose another folder. A folder that you back up is a good choice.
2. **Vault name** is optional.
3. Type a **Master password** of at least 12 characters, then type it again in **Confirm password**.
4. Select **Create vault**.

**Write down your master password and keep it somewhere safe.** Nobody can reset it. If you lose it, you lose the keys in the vault.

## Add a key

1. Select **Add key**.
2. Type a **Title**, and paste the secret value into the secret field. A title such as "Test key" is enough.
3. To use the key in a project later, fill in **Project**, **Environment** and **Environment variable**. For example: `my-app`, `dev` and `OPENAI_API_KEY`.
4. To get reminders, fill in **Expires** or **Rotate every**.
5. Select **Add key** at the bottom of the form.

Do you already have a `.env` file? Open **Use keys** and select **Import .env…**. AKM shows the names for you to confirm, and saves nothing until you confirm.

## Use a key

**One key.** Open **Keys** and select the key. Select **Copy** to put the secret on the clipboard, or **Reveal** to see it. AKM hides a revealed secret after 30 seconds and tries to clear its copy from the clipboard after 30 seconds.

**All the keys of a project.** Open **Use keys**, choose a project and an environment, and select **Save .env…**. The file holds your secrets as plain text. Keep it out of Git and delete it when you are done.

## Every day

- AKM locks after 10 minutes without activity, and when Windows locks or sleeps. Select **Lock now** to lock it at once.
- Open AKM and type your master password to unlock it.
- Keys that are overdue or expire soon show at the top of **Keys**.
- A key that you delete goes to the **Recycle Bin** first.
- **Activity** lists what happened to your keys, on this computer.

## Keep your keys safe

- Copy your vault file to a second place from time to time, for example a USB drive. AKM keeps five backups next to the vault, but a disk failure can take them all.
- Your vault is a standard KeePass file. KeePassXC can open it, so your keys do not depend on AKM.
- If you keep the vault in OneDrive, Dropbox or Syncthing, keep the file downloaded, and do not edit it on two computers at once.

## Remove AKM

Copy your vault file to a safe place first. Then open Windows **Settings**, go to **Apps**, then **Installed apps**, find **AKM** and select **Uninstall**.

The uninstaller removes the program. It keeps your vault and the activity journal (`%LOCALAPPDATA%\akm\journal\`). If you tick **Delete app data**, it also removes the app's settings and window data (`%APPDATA%\com.akm.desktop` and `%LOCALAPPDATA%\com.akm.desktop`), with everything in those folders. Do not keep your vault there.

## More detail

The [README](../README.md) describes what AKM protects and what it does not protect, and how developers can use its command-line tool.
