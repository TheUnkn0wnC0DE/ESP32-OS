# Security Policy

## Supported versions

Only the latest release receives fixes. Update via *Settings → System → Check for updates* or the [latest release](https://github.com/TheUnkn0wnC0DE/ESP32-OS/releases/latest).

| Version | Supported |
|---|---|
| 1.0.x (latest) | Yes |
| < 1.0 | No |

## Reporting a vulnerability

Please **do not** report security problems in public issues or discussions.

Report them privately through GitHub: open the [Security tab](https://github.com/TheUnkn0wnC0DE/ESP32-OS/security) and click **Report a vulnerability**.

Please include:

- ESP32-OS version and chip revision of the board
- Which part is affected (for example firmware update, browser/HTTPS, Wi-Fi, file handling)
- Steps to reproduce or a proof of concept
- What an attacker could achieve

You will get a first answer within 14 days. ESP32-OS is a hobby project maintained by one person, so fixes can take some time; you will be kept informed and credited in the release notes if you want.

## Relevant areas

Especially interesting are problems in:

- the firmware update over Wi-Fi (download from GitHub releases, SHA-256 check, rollback)
- the browser (NetSurf, QuickJS-ng, HTTPS certificate check)
- network code and handling of files from the microSD card

## Known limitations

These are documented and not considered new vulnerabilities:

- Wi-Fi passwords are stored unencrypted in flash (no flash encryption, no eFuses).
- All programs share one address space. A faulty program is stopped, but can still disturb the system.
- Anyone with physical access to the USB port can flash new firmware.
