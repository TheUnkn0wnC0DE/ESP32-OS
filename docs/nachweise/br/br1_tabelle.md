# BR1 – 30 Seiten am Board (25.11.2026)

Je Seite ein frischer Browser, 60 s, Firmware 1.0.3-Vorbereitung.

| # | Seite | Abrufe (Status) | Abruf-Fehler | Heap Spitze/Grösse KB | intern danach KB | Browser vorzeitig weg | Absturz | Ergebnis | Grund |
|---|---|---|---|---|---|---|---|---|---|
| 1 | https://de.wikipedia.org/wiki/Schweiz | 2xx: 16 | HTML nach 512 KB abgeschnitten (Speicher): https://de.wikipe | 10372/10628 | 217 | nein | nein | abgeschnitten | HTML > 512 KB |
| 2 | https://www.google.com/ | 2xx: 10, 3xx: 1 | – | 4765/10628 | 217 | nein | nein | geladen |  |
| 3 | https://duckduckgo.com/ | 2xx: 18 | – | 5598/10628 | 217 | nein | nein | geladen |  |
| 4 | https://github.com/ | 2xx: 29 | HTML nach 512 KB abgeschnitten (Speicher): https://github.co | 10372/10628 | 216 | nein | nein | abgeschnitten | HTML > 512 KB |
| 5 | https://www.srf.ch/ | 2xx: 53 | ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: data.sr | 9040/10628 | 212 | nein | nein | geladen | Tracker/Werbung nicht erreichbar |
| 6 | https://www.20min.ch/ | 2xx: 75 | ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: dev.vis; ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: funding; HTML nach 512 KB abgeschnitten (Speicher): https://www.20min | 10370/10628 | 216 | nein | nein | abgeschnitten | HTML > 512 KB |
| 7 | https://www.blick.ch/ | 2xx: 1, 4xx: 2 | – | 2945/10628 | 216 | nein | nein | geladen |  |
| 8 | https://www.digitec.ch/ | 2xx: 5, 4xx: 1 | – | 3142/10628 | 216 | nein | nein | geladen |  |
| 9 | https://www.galaxus.ch/ | 2xx: 5, 4xx: 1 | – | 3142/10628 | 215 | nein | nein | geladen |  |
| 10 | https://www.sbb.ch/ | 2xx: 1, 4xx: 13 | – | 3030/10628 | 215 | nein | nein | geladen | viele 4xx (Bot-Sperre?) |
| 11 | https://www.meteoschweiz.admin.ch/ | 2xx: 83 | HTML nach 512 KB abgeschnitten (Speicher): https://www.meteo | 10183/10628 | 215 | nein | nein | abgeschnitten | HTML > 512 KB |
| 12 | https://www.reddit.com/ | 2xx: 3 | – | 3114/10628 | 215 | nein | nein | geladen |  |
| 13 | https://news.ycombinator.com/ | 2xx: 7 | – | 4824/10628 | 215 | nein | nein | geladen |  |
| 14 | https://www.bbc.com/news | 2xx: 68 | – | 10372/10628 | 211 | nein | nein | geladen |  |
| 15 | https://stackoverflow.com/questions | 2xx: 4, 3xx: 1, 4xx: 1 | – | 4460/10628 | 214 | nein | nein | geladen |  |
| 16 | https://www.amazon.de/ | 2xx: 3 | – | 2901/10628 | 214 | nein | nein | geladen |  |
| 17 | https://www.spiegel.de/ | 2xx: 7 | – | 7782/10628 | 214 | nein | nein | geladen |  |
| 18 | https://www.heise.de/ | 2xx: 37 | HTML nach 512 KB abgeschnitten (Speicher): https://www.heise | 10372/10628 | 214 | nein | nein | abgeschnitten | HTML > 512 KB |
| 19 | https://www.tagesanzeiger.ch/ | 2xx: 53, 3xx: 1 | HTML nach 512 KB abgeschnitten (Speicher): https://www.tages | – | – | ja | nein | Browser beendet | Heap voll (10,6 MB) |
| 20 | https://www.nzz.ch/ | 2xx: 48 | HTML nach 512 KB abgeschnitten (Speicher): https://www.nzz.c | 10372/10628 | 213 | nein | nein | abgeschnitten | HTML > 512 KB |
| 21 | https://www.admin.ch/ | 2xx: 2, 4xx: 1 | – | 2945/10628 | 213 | nein | nein | geladen |  |
| 22 | https://www.ricardo.ch/ | 2xx: 42, 3xx: 2 | ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: btloade; ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: securep; ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: swissmg | 9800/10628 | 210 | nein | nein | geladen | Tracker/Werbung nicht erreichbar |
| 23 | https://www.tutti.ch/ | 2xx: 46, 3xx: 1 | – | – | – | ja | nein | Browser beendet | Heap voll (10,6 MB) |
| 24 | https://www.youtube.com/ | 2xx: 8 | HTML nach 512 KB abgeschnitten (Speicher): https://www.youtu | 4612/10628 | 213 | nein | nein | abgeschnitten | HTML > 512 KB |
| 25 | https://www.wikipedia.org/ | 2xx: 14, 4xx: 1 | – | 4789/10628 | 212 | nein | nein | geladen |  |
| 26 | https://developer.mozilla.org/de/ | 2xx: 30 | – | 5802/10628 | 212 | nein | nein | geladen |  |
| 27 | https://docs.python.org/3/ | 2xx: 24 | ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: analyti | 3526/10628 | 212 | nein | nein | geladen | Tracker/Werbung nicht erreichbar |
| 28 | https://www.w3.org/ | 2xx: 28 | – | 5478/10628 | 212 | nein | nein | geladen |  |
| 29 | https://www.openstreetmap.org/ | 2xx: 13 | ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: matomo. | 7652/10628 | 212 | nein | nein | geladen | Tracker/Werbung nicht erreichbar |
| 30 | https://www.apple.com/ch-de/ | 2xx: 16, 4xx: 2 | – | 7765/10628 | 212 | nein | nein | geladen |  |

Neustarts der Firmware im Lauf (ohne den ersten): 0

Ergebnis (nach Log; Fotos nur der ersten 6 Seiten): geladen 21, abgeschnitten 7, Browser beendet 2
