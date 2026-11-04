# U1 round 2: update from dictation-app to Vocet (VM evidence)

Scenario `e2e_update_from_dictation_app` (tests/e2e/scenarios), dictation-vm, 2026-11-04, PASS 2/2 with `e2e_onboarding_first_run`. Old build: `b7ae67e` (dictation-app.app 0.1.0); new: this branch (0.1.1); both signed `dictation-app Dev`, bundle id `com.dictationapp.desktop`, throwaway updater key. Raw evidence (Background Task Management dump, log lines):

```
BTM before the update:
       Developer Name: (null)
                 Type: legacy agent (0x10008)
                Flags: [ legacy ] (0x1)
          Disposition: [enabled, allowed, visible, not notified] (0x3)
           Identifier: 8.com.dictationapp.cua-driver-autostart
                  URL: file:///Users/lume/Library/LaunchAgents/com.dictationapp.cua-driver-autostart.plist
      Executable Path: /usr/bin/open
           Generation: 0
    Parent Identifier: Unknown Developer

 #3:
                 UUID: F7C6D368-E3BE-4690-A4EA-C3B58168CD15
                 Name: dictation-app
       Developer Name: (null)
                 Type: app (0x2)
                Flags: [  ] (0)
          Disposition: [enabled, allowed, visible, notified] (0xb)
           Identifier: 2.com.dictationapp.desktop
                  URL: file:///Users/lume/Applications/dictation-app.app/
           Generation: 1
    Bundle Identifier: com.dictationapp.desktop
login item (from dictation-app.app): launch at login: enabled
after the in-app update, ~/Applications holds: /Users/lume/Applications/dictation-app.app 
reading the key after the update: 2026-11-04T09:36:54.267515Z  WARN dictation_app_lib::engine: could not read the API key from the keychain error=keychain error: no answer from the keychain after 15 s (locked, or waiting for access to be allowed)
2026-11-04T09:36:54.268862Z  INFO dictation_app_lib::engine: engine settings provider=groq model=whisper-large-v3 language=Some("en") has_key=false
login item (from dictation-app.app): launch at login: enabled
BTM after the in-app update:
       Developer Name: (null)
                 Type: legacy agent (0x10008)
                Flags: [ legacy ] (0x1)
          Disposition: [enabled, allowed, visible, not notified] (0x3)
           Identifier: 8.com.dictationapp.cua-driver-autostart
                  URL: file:///Users/lume/Library/LaunchAgents/com.dictationapp.cua-driver-autostart.plist
      Executable Path: /usr/bin/open
           Generation: 0
    Parent Identifier: Unknown Developer

 #3:
                 UUID: F7C6D368-E3BE-4690-A4EA-C3B58168CD15
                 Name: dictation-app
       Developer Name: (null)
                 Type: app (0x2)
                Flags: [  ] (0)
          Disposition: [enabled, allowed, visible, notified] (0xb)
           Identifier: 2.com.dictationapp.desktop
                  URL: file:///Users/lume/Applications/dictation-app.app/
           Generation: 1
    Bundle Identifier: com.dictationapp.desktop
login item name in Login Items:                  Name: dictation-app
after the DMG-style move, ~/Applications holds: /Users/lume/Applications/Vocet.app 
login item (from Vocet.app): launch at login: enabled
BTM after the DMG-style move:
       Developer Name: (null)
                 Type: legacy agent (0x10008)
                Flags: [ legacy ] (0x1)
          Disposition: [enabled, allowed, visible, not notified] (0x3)
           Identifier: 8.com.dictationapp.cua-driver-autostart
                  URL: file:///Users/lume/Library/LaunchAgents/com.dictationapp.cua-driver-autostart.plist
      Executable Path: /usr/bin/open
           Generation: 0
    Parent Identifier: Unknown Developer

 #3:
                 UUID: F7C6D368-E3BE-4690-A4EA-C3B58168CD15
                 Name: dictation-app
       Developer Name: (null)
                 Type: app (0x2)
                Flags: [  ] (0)
          Disposition: [enabled, allowed, visible, notified] (0xb)
           Identifier: 2.com.dictationapp.desktop
                  URL: file:///Users/lume/Applications/Vocet.app/
           Generation: 2
    Bundle Identifier: com.dictationapp.desktop
login item name in Login Items:                  Name: dictation-app
```
