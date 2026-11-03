# dictation-app: tester guide

Hold a key, speak, let go, and your words are typed into whatever app is in
front. Thank you for trying it. This page takes you from the DMG to your first
dictation, and says what to send us when something goes wrong.

You need an **Apple Silicon Mac (M1 or newer; Intel Macs are not supported)**
with **macOS 13 (Ventura) or newer**, an internet connection (your speech goes
to Groq or OpenAI to be turned into text) and an API key for one of them. Groq
has a free tier and is the one this guide uses; OpenAI works the same way.

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
without a Dock entry), not in the Dock. The first time it opens, a **Set up
dictation** window walks you through four steps. If you don't see it, click
the menu-bar icon.

## 3. First run: the Set up dictation window

The window shows "Step 1 of 4" and so on, and has **Back** and **Continue**
buttons. A permission step moves on by itself once you have allowed it.

1. **Allow the microphone.** Click **Allow microphone**; macOS asks once, so
   choose **Allow**. dictation-app records only while you hold the hotkey. If
   you chose "Don't Allow" earlier, the step says the microphone is turned off:
   click **Open System Settings** and switch **dictation-app** on under
   **Privacy & Security > Microphone**.
2. **Allow Accessibility.** The hotkey and pasting need it. Click **Open
   System Settings** and switch **dictation-app** on under **Privacy &
   Security > Accessibility**. If it is not in the list, click **+**, choose
   `/Applications/dictation-app.app` and switch it on. If the step doesn't move
   on after that, click **Relaunch**; the app opens again at this step.
3. **Add your API key.** Pick the **Provider** (**Groq** is the recommended
   one; **OpenAI** is the alternative), then paste your key into the key box.
   - Groq: go to <https://console.groq.com/keys>, sign up (free; Google
     sign-in works), click **Create API Key**, give it any name and copy it.
     It starts with `gsk_`. You see it only once.
   - OpenAI: create a key at <https://platform.openai.com/api-keys>
     (needs a billing-enabled account).

   Click **Test and continue**. The key is tested first; a wrong one shows
   the reason and nothing is saved. A good one is kept in your macOS keychain
   and never shown again. The key is the only secret: it stays on your Mac and
   is sent only to the provider you chose.

4. **Try it.** Hold **the right Option key** (right of the space bar), say a
   few words and let go. The text appears in the box in the window. When it
   does, the window says "It works." Click **Done**.

Both permissions are kept when you update to a newer build that we sign the
same way. If a new build ever asks again, grant it again and report it (see "Report a bug").

### Changing things later: Settings

Menu-bar icon > **Settings…** is also where you go to fix things, and to
change anything you set above:

- **Provider** and **Groq/OpenAI API key**: paste a new key, click **Test
  key** (it says _"The key works. Groq accepted it."_; a wrong key says _"The
  provider rejected your API key"_), then **Save**. The box then says **Key
  saved**.
- **Hotkey**, **Language** and **Launch at login**.
- A permission turned off by mistake: switch it back on in **System Settings >
  Privacy & Security** (Microphone or Accessibility), then quit dictation-app
  from its menu-bar icon and open it again.

## 4. Dictate

1. Click into any text field (Notes, Slack, a browser, a document).
2. **Hold the right Option key**, speak, and let go. A small overlay shows
   that it is recording and then that it is working.
3. The text appears where your cursor was, a moment later.

Change the key with the **Hotkey** box in Settings. Password fields are
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
  update key, so it was not installed."_). If you see that, report it (see "Report a bug").

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
  and attach the zip to a bug report (see "Report a bug" below; the files are
  small: at most 10 MB in all).
- **If we ask for more detail:** quit the app, then in Terminal run
  `open --env DICTATION_LOG=debug /Applications/dictation-app.app`, repeat the
  problem, and send the log again. Debug logs show how long a dictation was,
  still not its text.

## Report a bug

Open an issue: <https://github.com/gargankit90/dictation-app/issues/new/choose>
and pick **Bug report**. The form asks for:

- your macOS version (Apple menu > **About This Mac**) and Mac model,
- the dictation-app version: menu-bar icon > **Check for updates…** says
  "dictation-app _x.y.z_ is the newest version" (or what you have, if there is
  an update),
- what you did, what you expected and what happened,
- the log zip from above. The log never holds your API key or what you said
  (keys are removed before it is written; dictated text is not written at all),
  but the issue is public, so open the zip and check it before you attach it.

Never paste an API key into an issue. For a security problem, don't open a
public issue; see [SECURITY.md](../SECURITY.md).

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

  4. Remove the saved API key: open **Keychain Access**, search for
     `com.dictationapp.desktop` and delete that entry. (You can also delete the
     key itself on your provider's key page: Groq or OpenAI, as in section 3.)
  5. In **System Settings > Privacy & Security**, remove dictation-app from
     **Microphone** and **Accessibility** (select it, click **−**).

## Known limits

- Apple Silicon Macs only, macOS 13 or newer. Windows comes later.
- Not notarised, hence the step in section 2.
- Speech goes to your chosen provider's servers (Groq or OpenAI) while the alpha uses the cloud. Don't dictate
  anything you would not send to a cloud service.
- Recordings stop after 5 minutes.
