# dictation-app: tester guide

Hold a key, speak, let go, and your words are typed into whatever app is in
front. Thank you for trying it. This page takes you from the DMG to your first
dictation, and says what to send us when something goes wrong.

You need a Mac with **macOS 13 (Ventura) or newer**, an internet connection
(your speech goes to Groq to be turned into text) and a free Groq account.

## 1. Install

1. Download the DMG (`dictation-app_<version>_aarch64.dmg`) from the newest
   release at
   <https://github.com/gargankit90/dictation-app/releases/latest>,
   or use the one you were sent, and open it.
2. Drag **dictation-app** onto the **Applications** shortcut in the window.
3. Eject the DMG (Finder sidebar, or drag it to the Trash).

The licences of the open-source software inside the app are on the release
page as `THIRD-PARTY-LICENSES.txt`, and inside the app at
`dictation-app.app/Contents/Resources/`.

## 2. Open it for the first time (clear the quarantine flag)

dictation-app is not yet signed with an Apple Developer ID, so macOS blocks
the first launch of anything downloaded from a browser, chat or email with
_"dictation-app can't be opened because Apple cannot check it for malicious
software"_. You tell macOS you trust it once. Open **Terminal** (Spotlight:
`Terminal`) and paste this. It assumes you already dragged the app into
**Applications** (section 1); if you opened it from the DMG window instead,
do that first, or the command says "No such file":

```sh
xattr -dr com.apple.quarantine /Applications/dictation-app.app
```

It prints nothing when it works. Then open **dictation-app** from Applications.

If you would rather not use Terminal: try to open the app, click **Done** on
the warning, then go to **System Settings > Privacy & Security**, scroll down
to _"dictation-app was blocked…"_ and click **Open Anyway**.

dictation-app lives in the **menu bar** (top right of the screen, an icon
without a Dock entry). If you see no window, that is normal: click the icon
and choose **Settings…**.

## 3. Give it the two permissions

macOS asks for each one the first time it is needed. Allow both, or dictation
can't work.

| Permission        | What it is for                          | Where to grant it                                            |
| ----------------- | --------------------------------------- | ------------------------------------------------------------ |
| **Microphone**    | Hearing you while the hotkey is held    | A prompt appears at your first dictation: click **Allow**.   |
| **Accessibility** | Seeing the hotkey and pasting the text  | **System Settings > Privacy & Security > Accessibility**: switch **dictation-app** on. |

For Accessibility, macOS shows a prompt with a button that opens that screen.
If you don't see dictation-app in the list, click **+**, choose
`/Applications/dictation-app.app` and switch it on. After switching it on,
**Quit dictation-app from its menu-bar icon and open it again** so it picks
the permission up.

Both are kept when you update to a newer build that we sign the same way. If
a new build ever asks again, grant it again and tell us.

## 4. Get a Groq key and paste it in

1. Go to <https://console.groq.com/keys> and sign up (free; Google sign-in
   works).
2. Click **Create API Key**, give it any name and copy the key. It starts
   with `gsk_`. You see it only once.
3. Click the dictation-app icon in the menu bar > **Settings…**.
4. Paste the key into **Groq API key**.
5. Click **Test key**. It should say _"The key works. Groq accepted it."_ A
   wrong key says _"The provider rejected your API key"_.
6. Click **Save**. The box then says **Key saved**. The key is kept in your
   macOS keychain and is never shown again; to replace it, paste a new one.

The key is the only secret. It stays on your Mac and is sent only to Groq.

## 5. Dictate

1. Click into any text field (Notes, Slack, a browser, a document).
2. **Hold the right Option key** (the Option key right of the space bar),
   speak, and let go. A small overlay shows that it is recording and then that
   it is working. The first time, macOS asks to use the microphone: click
   **Allow**, then dictate again.
3. The text appears where your cursor was, a moment later.

You can change the key in Settings (the **Hotkey** box). Password fields are
skipped on purpose: nothing is pasted there.

If nothing happens, see "Something went wrong" below.

## How updates arrive

You install from the DMG once. After that, dictation-app updates itself:

- **When it starts**, it quietly asks the release page whether there is a
  newer build. If there is, it shows **Update available** with what is new;
  if not, or if you are offline, it shows nothing.
- **Any time**, click the menu-bar icon > **Check for updates…**. It says
  _"You're up to date"_, offers the update, or says plainly that it couldn't
  check (_"Couldn't check for updates"_, for example when you are offline).
- Nothing is downloaded or installed until you click **Install and
  restart**. **Later** keeps the build you have; you are asked again next
  time.
- The app restarts as the new version by itself. **Microphone and
  Accessibility stay granted**: every build is signed the same way, so macOS
  knows it is the same app. You don't clear the quarantine flag again.
- Every update is signed with our update key, and the app refuses one whose
  signature doesn't match (_"The update's signature didn't match this app's
  update key, so it was not installed."_). If you see that, tell us.

If an update ever goes wrong, download the newest DMG from the release page
and install it over the old app, as in section 1.

## Something went wrong: the log

dictation-app writes a log that you can send us. It holds what the app did
(starts, recordings, errors), not what you said, and **never your API key**:
keys are removed from the log before it is written, and the words you dictate
are not written at all.

- **See it:** menu-bar icon > **Reveal log** opens the folder in Finder.
  It is `~/Library/Logs/com.dictationapp.desktop/`.
- **Send it:** in that folder, select all the files, right-click > **Compress**,
  and send us the zip (the files are small: at most 10 MB in all).
- **If we ask for more detail:** quit the app, then in Terminal run
  `open --env DICTATION_LOG=debug /Applications/dictation-app.app`, repeat the
  problem, and send the log again. Debug logs show how long a dictation was,
  still not its text.

Please also tell us what you did, what you expected, what happened, your
macOS version and the dictation-app version (the DMG's name, or the
**Update available** dialog's "you have …").

## Quit and uninstall

- **Quit:** menu-bar icon > **Quit dictation-app**. (Closing the Settings
  window only hides it; the app keeps running in the menu bar.)
- **Stop it starting at login:** untick **Launch at login** in Settings and
  **Save**, or remove it in System Settings > General > Login Items.
- **Uninstall:**
  1. Quit the app as above.
  2. Drag `/Applications/dictation-app.app` to the Trash.
  3. Remove its data (Terminal):

     ```sh
     rm -rf ~/Library/Logs/com.dictationapp.desktop \
            ~/Library/Application\ Support/com.dictationapp.desktop
     ```

  4. Remove the saved Groq key: open **Keychain Access**, search for
     `com.dictationapp.desktop` and delete that entry. (You can also delete the
     key itself at <https://console.groq.com/keys>.)
  5. In **System Settings > Privacy & Security**, remove dictation-app from
     **Microphone** and **Accessibility** (select it, click **−**).

## Known limits

- Mac only, macOS 13 or newer. Windows comes later.
- Not notarised, hence the step in section 2.
- Speech goes to Groq's servers while the alpha uses the cloud. Don't dictate
  anything you would not send to a cloud service.
- Recordings stop after 5 minutes.
