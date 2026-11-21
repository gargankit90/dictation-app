# Vocet: tester guide

Hold a key, speak, let go, and your words are typed into whatever app is in
front. Thank you for trying it. This page takes you from the DMG to your first
dictation, and says what to send us when something goes wrong.

What is in each version is in the [changelog](../CHANGELOG.md).

You need an **Apple Silicon Mac (M1 or newer; Intel Macs are not supported)**
with **macOS 13 (Ventura) or newer**, and one of two ways to turn speech into
text:

- **OpenRouter** (the default): an internet connection and an OpenRouter account (you
  pay OpenRouter for what you use; Connect OpenRouter makes the key). Your speech goes through OpenRouter to
  OpenAI for transcription, and the text to a cleanup model (with "ZDR only" on in your OpenRouter account it is a zero-data-retention host). Vocet has no server
  of its own that receives it (policy: vocetlabs.com/privacy).
- **On this Mac**: one download (about 4.4 GB: English speech recognition,
  then a cleanup model that adds punctuation and removes fillers). Your
  speech then stays on the Mac. It is a choice in setup, not the default.

## 1. Install

1. Download the DMG (`Vocet_<version>_aarch64.dmg`) from the newest
   release at
   <https://github.com/gargankit90/dictation-app/releases/latest>,
   or use the one you were sent, and open it.
2. Drag **Vocet** onto the **Applications** shortcut in the window.
3. Eject the DMG (Finder sidebar, or drag it to the Trash).

The licences of the open-source software inside the app are on the release
page as `THIRD-PARTY-LICENSES.txt`, and inside the app at
`Vocet.app/Contents/Resources/`.

<!-- SWITCH AT THE FIRST NOTARIZED RELEASE (N2, docs/releasing.md): this is the
notarized version of section 2. The old Terminal steps are gone; releases are
Developer ID-signed and notarized from the first one. -->

## 2. Open it for the first time

Open **Vocet** from Applications. macOS checks it, and asks once whether you
are sure you want to open an app you downloaded (_"Vocet is an app created by
the app Safari. Are you sure you want to open it?"_, or "downloaded from the
Internet", with "Apple checked it for malicious software and none was
detected"): click **Open**. Vocet is signed with an Apple Developer ID and
notarized by Apple, so that is all.

If macOS still refuses to open it, tell us in a bug report (see "Report a bug" below) with the
exact words it shows.

Vocet lives in the **menu bar** (top right of the screen, an icon
without a Dock entry), not in the Dock. The first time it opens, a setup window walks you through five steps. If you
don't see it, click the menu-bar icon.

## 3. First run: the setup window

The window has a list of steps on the side and **Continue** and **Back**
buttons. A permission step moves on by itself once you have allowed it.

1. **Welcome.** Click **Get started**.
2. **Speech.** Choose how to turn speech into text (Vocet 0.2 is for
   English):
   - **Use OpenRouter** (selected by default): click **Continue**, then **Connect OpenRouter**.
     Your browser opens OpenRouter; sign in and allow it. A new account needs
     a few dollars of credit: **Add credits** opens that page. The key
     OpenRouter makes is kept in your macOS Keychain and never shown in
     full. If the browser can't come back, **Paste a key instead**. It is sent only to
     OpenRouter.
   - **Use this Mac** (not selected by default): select it, then click
     **Download and continue**. One download of about 4.4 GB (speech, then
     cleanup) with one progress bar; it keeps going while you do the next
     steps.
3. **Permissions.** Click **Allow** for the **Microphone** (macOS asks once;
   choose **Allow**) and for **Accessibility** (the dictation key and typing
   into other apps need it). For Accessibility, switch **Vocet** on under
   **System Settings > Privacy & Security > Accessibility**; if it isn't in
   the list, click **+** and choose `/Applications/Vocet.app`. If the step
   doesn't move on afterwards, click **Relaunch and resume**. A "Turned off"
   badge means you refused it earlier: switch it on in System Settings.
4. **Try it.** Your dictation key is shown (the right Option key, right of the
   space bar; **Change…** picks another). Hold it, say a sentence and let go.
   The words appear in the box. Click **Done**.
5. **You're set.** A summary of your key, speech and history. Here you can
   choose **Open Vocet when I log in** and the **Share usage and error
   events** switch (see "Privacy" below). Click **Start dictating**.

### Changing things later: Settings

Menu-bar icon > **Settings…** has these sections:

- **General**: open Vocet at login, and **Check for updates** (**Check now**).
- **Dictation**: the **dictation key** (hold it to talk, release to paste),
  and where the microphone is chosen (System Settings → Sound).
- **Speech**: the list of engines. Select a row to use it: **OpenRouter**
  (first; its own sheet: **Connect OpenRouter**, **Add credits**,
  **Disconnect**), or **On this Mac** (download it first: one download of
  speech and cleanup models, about 4.4 GB, with one progress bar and one
  **Pause**).
- **Privacy**: the **Share usage and error events** switch.
- **History**: your recent dictations, with **Copy** and **Delete** on each,
  how many to **Keep** (10, 25, 50 or 100, or off) and **Clear history…**.
  Text only, kept on this Mac, never sent.
- **Help**: **Troubleshoot**, **Report a problem**, **Log file** and **Run
  setup again**.
- **About**: the version, the licence, the source code and credits.

A permission turned off by mistake: switch it back on in **System Settings >
Privacy & Security** (Microphone or Accessibility). Vocet tells you in the
setup window when macOS has taken one away.

### The menu-bar icon

Copy last dictation, **History…**, **Settings…**, **Help…** and **Quit
Vocet**. If something is wrong (a missing permission, a rejected key), the
menu offers the fix (for example **Open Microphone settings…**).

## 4. Dictate

1. Click into any text field (Notes, Slack, a browser, a document).
2. **Hold the right Option key**, speak, and let go. A small overlay shows
   that it is recording and then that it is working.
3. A moment later, the text is typed into whichever app has focus when it is
   pasted.

Change the key with **Settings > Dictation**. Password fields are skipped on
purpose: nothing is pasted there. If a dictation didn't get pasted, find it in
**History…** and copy it. Don't press other keys while you hold the key (see
"Known limits").

If nothing happens, see "Something went wrong" below.

## How updates arrive

You install from the DMG once. After that, Vocet updates itself:

- **When it starts**, it quietly asks the release page whether there is a
  newer build. If there is, it shows **Update available** with what is new;
  if not, or if you are offline, it shows nothing.
- **Any time**, click the menu-bar icon > **Settings…** > General > **Check
  now**. It says _"You're up to date"_, offers the update, or says plainly
  that it couldn't check (for example when you are offline).
- Nothing is downloaded or installed until you click **Install and
  restart**. **Later** keeps the build you have; you are asked again next
  time.
- The app restarts as the new version by itself. **Microphone and
  Accessibility stay granted**: every build is signed the same way, so macOS
  knows it is the same app, and an update never asks whether to open it. (The
  one exception is an update whose notes say it is signed differently: Vocet
  then opens its setup window and shows you how to allow both again. Your
  settings, history and saved key stay.)
- **macOS may ask once whether Vocet can use the key saved in your Keychain.
  Choose Always Allow.** Your key stays in your Keychain and is sent only to
  OpenRouter. The prompt reads "Vocet wants to
  access key com.dictationapp.desktop"; enter your Mac password. Until you
  answer, Vocet can't read the key and says so. It can come back after any
  update.
- Every update is signed with our update key, and the app refuses one whose
  signature doesn't match (_"The update's signature didn't match this app's
  update key, so it was not installed."_). If you see that, report it (see
  "Report a bug").

If an update ever goes wrong, download the newest DMG from the release page
and install it over the old app, as in section 1.

## Privacy

- **Your words.** With **On this Mac**, your speech is turned into text on the
  Mac and not sent anywhere. With **OpenRouter**, it goes through OpenRouter.
  Your history stays on this Mac. Audio stays in memory.
- **Usage and error events (random install ID) are on by default.** Turn them off with
  **Share usage and error events** in Settings > Privacy (or on the last setup
  screen). Off takes effect at once and deletes what was waiting. Usage events carry
  no audio, no text you dictated, no API keys and no app names. A crash report includes the app version, the macOS major version, an error code and a stack trace: function names and source-file locations from Vocet, the libraries it uses and the Rust standard library, as they were when the app was built. It never includes the error message, paths to your files, your audio, dictated text, keys or app names.
  Everything that can be sent is listed in [privacy.md](privacy.md).

## Something went wrong: the log

Vocet writes a log that you can send us. It holds what the app did
(starts, recordings, errors), not what you said, and **never your API key**:
keys are removed from the log before it is written, and the words you dictate
are not written at all.

- **See it:** menu-bar icon > **Help…** > Log file > **Show in Finder** opens the folder
  (or Settings > Help).
  It is `~/Library/Logs/com.dictationapp.desktop/`.
- **Send it:** in that folder, select all the files, right-click > **Compress**,
  and attach the zip to a bug report (see "Report a bug" below; the files are
  small: at most 10 MB in all).
- **If we ask for more detail:** quit the app, then in Terminal run
  `open --env DICTATION_LOG=debug /Applications/Vocet.app`, repeat the
  problem, and send the log again. Debug logs show how long a dictation was,
  still not its text.

## Report a bug

Open an issue: <https://github.com/gargankit90/dictation-app/issues/new/choose>
and pick **Bug report**. The form asks for:

- your macOS version (Apple menu > **About This Mac**) and Mac model,
- the Vocet version: menu-bar icon > **Settings…** > About shows
  the version,
- what you did, what you expected and what happened,
- the log zip from above. The log never holds your API key or what you said
  (keys are removed before it is written; dictated text is not written at all),
  but the issue is public, so open the zip and check it before you attach it.

Never paste an API key into an issue. For a security problem, don't open a
public issue; see [SECURITY.md](../SECURITY.md).

## Quit and uninstall

- **Quit:** menu-bar icon > **Quit Vocet**. (Closing the Settings
  window only hides it; the app keeps running in the menu bar.)
- **Stop it starting at login:** switch off **Open Vocet at login** in Settings >
  General (it applies at once), or remove it in System Settings > General > Login Items.
- **Uninstall:**
  1. Quit the app as above.
  2. Drag `/Applications/Vocet.app` to the Trash.
  3. Remove its data (Terminal):

     ```sh
     rm -rf ~/Library/Logs/com.dictationapp.desktop \
            ~/Library/Application\ Support/com.dictationapp.desktop
     ```

  4. Remove the saved API key: open **Keychain Access**, search for
     `com.dictationapp.desktop` and delete that entry. (You can also delete the
     key itself on your key page on OpenRouter, as in section 3.)
  5. In **System Settings > Privacy & Security**, remove Vocet from
     **Microphone** and **Accessibility** (select it, click **−**).

## Known limits

The same list is in the [changelog](../CHANGELOG.md).

- Apple Silicon Macs only, macOS 13 or newer. Windows comes later.
- **Don't press other keys while you hold the dictation key.** If you do, the
  recording is dropped without a message
  ([#315](https://github.com/gargankit90/dictation-app/issues/315); fixed
  after 0.1.0).
- A long dictation on an on-device model can take a while, and has no time
  limit yet ([#316](https://github.com/gargankit90/dictation-app/issues/316)).
- Recordings stop after 5 minutes.
- Fully offline use of **On this Mac** is not yet checked on a Mac with no
  network.
- A crash inside the on-device engine is not reported (it could contain what
  you said): if the app quits on its own, tell us in a bug report
  ([#226](https://github.com/gargankit90/dictation-app/issues/226)).
- With **OpenRouter**, speech goes through OpenRouter to OpenAI, and the
  text to a cleanup model. Don't dictate anything you would not send to a cloud service.
- Usage and error events are on by default; turn them off in
  Settings > Privacy. What is sent: [privacy.md](privacy.md).
