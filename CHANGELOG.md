# Changelog

All notable changes to dictation-app are listed here, newest first. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The section
for a version is the text of its GitHub release and of the "Update available"
window (`scripts/release.sh` copies it, so keep each section plain text for
testers: short lines, no HTML).

## [Unreleased]

## [0.1.0] — 2026-11-12

Vocet is a dictation app for the Mac. Hold a key, speak, let go, and your
words are typed into the app you are using. This is the first release, for
testers. Thank you for trying it.

### What Vocet does

- **Dictate into any app.** Hold the right Option key, speak, and let go. A
  small overlay shows when Vocet is recording and when it is working. The
  text is typed where your cursor is. Password fields are skipped on purpose.
- **Change the dictation key** in Settings, Dictation.
- **Speech on your Mac.** Download a model once (Parakeet for English, about
  480 MB; Qwen3-ASR for about 30 languages, including Hindi, about 590 MB).
  After that your speech is turned into text on the Mac and not sent anywhere.
- **Or speech with your own key.** Choose Groq or OpenAI and paste your key.
  Your speech goes to that provider. The key is kept in your macOS Keychain
  and never shown again.

### What is in this release

- **Setup.** A first-run window picks your language and speech, asks for the
  Microphone and Accessibility permissions, and has you try a dictation.
- **History.** Vocet keeps your last 25 dictations on this Mac so you can get
  the words back. Text only, never sent. Change the number or turn it off in
  Settings, History.
- **Menu bar.** Copy last dictation, History, Settings, Help and Quit.
- **Settings.** General, Dictation, Speech, Privacy, History, Help and About.
- **Help.** Troubleshoot checks the microphone, Accessibility, the dictation
  key and the speech engine. Report a problem shows you the exact text of a
  report to save, copy or open as a GitHub issue. Vocet never sends it for
  you. The log file never holds your words or your keys.
- **Updates.** Vocet checks when it starts, and Settings, General, Check now
  does it any time. Nothing is installed until you click Install and restart.
  Every update is signed.
- **Anonymous usage sharing, on by default.** It sends which features work and
  which fail, never audio, text you dictated, keys, app names or file paths.
  Turn it off in Settings, Privacy, "Share usage and error events". Off takes
  effect at once. The full list is in
  https://github.com/gargankit90/dictation-app/blob/main/docs/privacy.md

### Requirements

- A Mac with Apple Silicon (M1 or newer), on macOS 13 (Ventura) or newer.
- For your own key: internet and a Groq or OpenAI API key.
- For speech on your Mac: disk space for the model you pick.

### Known limits

- Mac only. Windows comes later.
- Do not press other keys while you hold the dictation key. If you do, the
  recording is dropped without a message
  (https://github.com/gargankit90/dictation-app/issues/315).
- A long dictation on a model on your Mac can take a while, and has no time
  limit yet (https://github.com/gargankit90/dictation-app/issues/316).
- Recordings stop after 5 minutes.
- Using speech on your Mac with no network is built in but not yet checked.
- If the on-device engine crashes, Vocet does not report it, because a crash
  report could contain what you said. If Vocet quits on its own, tell us in a
  bug report
  (https://github.com/gargankit90/dictation-app/issues/226).
- With your own key, your speech goes to Groq or OpenAI. Do not dictate
  anything you would not send to a cloud service.

[Unreleased]: https://github.com/gargankit90/dictation-app/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/gargankit90/dictation-app/releases/tag/v0.1.0
