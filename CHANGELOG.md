# Changelog

All notable changes to dictation-app are listed here, newest first. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The section
for a version is the text of its GitHub release and of the "Update available"
window (`scripts/release.sh` copies it, so keep each section plain text for
testers: short lines, no HTML). The `[date set at release]` is filled in when
the release is cut.

## [Unreleased]

### Changed

- Simpler menu bar: Help… replaces Troubleshoot and Report a problem, and Check for updates moved to Settings, General.
- Speech is one list: select a row to choose the engine. Your own key has its own sheet.
- Dictation key: Change… replaces the recorder's buttons, and "dictation key" is the one word for it.
- Troubleshoot and Report a problem are separate screens, with error codes behind Details.
- Calmer buttons and sidebar selection in dark mode.

### Fixed

- Back from Troubleshoot or Report a problem returns to Help.
- If macOS takes a permission away, the setup window now says why, that your settings, history and key are still here, and closes once you have allowed it again.

### Added

- New: Vocet now keeps your last 25 dictations on this Mac so you can get the words back. Text only, never sent. Change or turn it off in Settings → History.

### Changed

- The app is now called **Vocet** (menu, windows, prompts). Your settings,
  saved key, permissions and login item carry over; the file in Applications
  may keep the name `dictation-app.app` after an in-app update.
- Errors are one plain sentence that names the fix, with no error code.
  The codes stay in Troubleshoot and the report.
- After this update, macOS may ask once whether Vocet can use the key saved in your Keychain. Choose Always Allow. Your key stays in your Keychain and is sent only to your speech provider (Groq or OpenAI).

## [0.1.0] — [date set at release]

The first release for testers. Hold a key, speak, let go, and your words are
typed into the app you are using.

### What you can do

- **Dictate into any app.** Hold the right Option key (you can change the key
  in Settings), speak, and let go. A moment later the text is typed into
  whichever app has focus when it is pasted. A small overlay shows when it is
  recording and when it is working. Password fields are skipped on purpose.
- **Transcribe on your Mac.** Pick "On this Mac" in Settings and download a
  model once. After that, your speech is turned into text on the Mac and not
  sent anywhere. Two models are offered: Parakeet (English, the default, about
  480 MB) and Qwen3-ASR (about 30 languages, including Hindi, about 590 MB).
- **Or use your own key.** Pick "Your own key", choose Groq or OpenAI and
  paste a key. Your speech goes to that provider to be transcribed. The key is
  kept in your macOS keychain and never shown again. The first-run window
  tests it before it moves on; in Settings, **Test key** and **Save** are
  separate buttons, so test it there before you rely on it.
- **Set it up in four steps.** A first-run window asks for the microphone and
  Accessibility permissions, your key (or "No key needed" with an on-device
  model) and a first test dictation.
- **Change things in Settings.** Hotkey, language, launch at login, the
  transcription choice and the usage-sharing switch.
- **Find out what went wrong.** Errors show a short code and what to do next.
  **Troubleshoot** (menu-bar icon, or Settings > Help) checks the microphone,
  Accessibility, the hotkey and the engine, and can test the connection to your
  provider. **Report a problem** shows you the exact text of a report, which
  you can save, copy or open as a GitHub issue. The app never sends the report
  itself.
- **Stay up to date.** The app asks at start-up whether there is a newer
  version, and **Check for updates…** in the menu-bar icon does it any time.
  Nothing is installed until you click **Install and restart**, and every
  update is signed.

### Privacy

- Anonymous usage and error events are **on by default**. Turn them off in
  Settings > Privacy > "Share usage and error events"; off takes effect at
  once. Events carry no audio, no text you dictated, no API keys, no app names
  and no file paths. Everything that can be sent is listed in
  [docs/privacy.md](docs/privacy.md).
- Dictated text is not written to the log, and audio stays in memory.

### Requirements

- An Apple Silicon Mac (M1 or newer). Intel Macs are not supported.
- macOS 13 (Ventura) or newer.
- For the "Your own key" choice: an internet connection and a Groq or OpenAI
  API key. For "On this Mac": enough disk space for the model you pick.

### Known limitations

- **Fully offline use is not yet checked.** On-device transcription is built
  to need no network once the model is downloaded, but we have not yet
  verified it on a Mac that is offline.
- **Crashes inside the on-device engine are not reported.** A crash report of
  that kind could contain what you said, so the app does not capture it
  ([#226](https://github.com/gargankit90/dictation-app/issues/226)). If the
  app quits on its own, tell us in a bug report. When sharing is on (and the
  build has error reporting), error reports cover only Rust panics and failed
  dictations; other crashes are not reported.
- Recordings stop after 5 minutes.
- Mac only. Windows comes later.

[Unreleased]: https://github.com/gargankit90/dictation-app/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/gargankit90/dictation-app/releases/tag/v0.1.0
