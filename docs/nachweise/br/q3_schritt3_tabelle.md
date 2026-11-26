| # | Seite | Abrufe (Status) | Abruf-Fehler | Heap Spitze/Gr�sse KB | intern danach KB | Browser vorzeitig weg | Absturz | Ergebnis | Grund |
|---|---|---|---|---|---|---|---|---|---|
| 1 | https://de.wikipedia.org/wiki/Schweiz | 2xx: 42 | HTML nach 512 KB abgeschnitten (Speicher): https://de.wikipe | 10372/10628 | 216 | nein | nein |  |  |
| 2 | https://www.google.com/ | 2xx: 10, 3xx: 1 | � | 7555/10628 | 216 | nein | nein |  |  |
| 3 | https://duckduckgo.com/ | 2xx: 18 | � | 7176/10628 | 216 | nein | nein |  |  |
| 4 | https://github.com/ | 2xx: 45 | HTML nach 512 KB abgeschnitten (Speicher): https://github.co | 10372/10628 | 216 | nein | nein |  |  |
| 5 | https://www.srf.ch/ | 2xx: 30 | � | 9616/10628 | 216 | nein | nein |  |  |
| 6 | https://www.20min.ch/ | 2xx: 71 | ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: dev.vis; ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: funding; HTML nach 512 KB abgeschnitten (Speicher): https://www.20min | 10372/10628 | 216 | nein | nein |  |  |
| 7 | https://www.blick.ch/ | 2xx: 1, 4xx: 2 | � | 2948/10628 | 215 | nein | nein |  |  |
| 8 | https://www.digitec.ch/ | 2xx: 5, 4xx: 1 | � | 3145/10628 | 215 | nein | nein |  |  |
| 9 | https://www.galaxus.ch/ | 2xx: 5, 4xx: 1 | � | 3145/10628 | 215 | nein | nein |  |  |
| 10 | https://www.sbb.ch/ | 2xx: 1, 4xx: 13 | � | 3096/10628 | 215 | nein | nein |  |  |
| 11 | https://www.meteoschweiz.admin.ch/ | 2xx: 95 | HTML nach 512 KB abgeschnitten (Speicher): https://www.meteo | 9274/10628 | 214 | nein | nein |  |  |
| 12 | https://www.reddit.com/ | 2xx: 3 | � | 3114/10628 | 214 | nein | nein |  |  |
| 13 | https://news.ycombinator.com/ | 2xx: 7 | � | 4847/10628 | 214 | nein | nein |  |  |
| 14 | https://www.bbc.com/news | 2xx: 68 | � | 10372/10628 | 210 | nein | nein |  |  |
| 15 | https://stackoverflow.com/questions | 2xx: 5, 3xx: 2, 4xx: 2 | � | 7405/10628 | 214 | nein | nein |  |  |
| 16 | https://www.amazon.de/ | 2xx: 3 | � | 2904/10628 | 213 | nein | nein |  |  |
| 17 | https://www.spiegel.de/ | 2xx: 7 | � | 7786/10628 | 213 | nein | nein |  |  |
| 18 | https://www.heise.de/ | 2xx: 38 | HTML nach 512 KB abgeschnitten (Speicher): https://www.heise | 10372/10628 | 213 | nein | nein |  |  |
| 19 | https://www.tagesanzeiger.ch/ | 2xx: 21, 3xx: 1 | HTML nach 512 KB abgeschnitten (Speicher): https://www.tages | 10372/10628 | 213 | nein | nein |  |  |
| 20 | https://www.nzz.ch/ | 2xx: 42 | HTML nach 512 KB abgeschnitten (Speicher): https://www.nzz.c | 10372/10628 | 213 | nein | nein |  |  |
| 21 | https://www.admin.ch/ | 2xx: 66, 4xx: 1 | � | 10223/10628 | 212 | nein | nein |  |  |
| 24 | https://www.youtube.com/ | 2xx: 8 | HTML nach 512 KB abgeschnitten (Speicher): https://www.youtu | 4617/10628 | 212 | nein | nein |  |  |
| 25 | https://www.wikipedia.org/ | 2xx: 13, 4xx: 1 | � | 6838/10628 | 212 | nein | nein |  |  |
| 26 | https://developer.mozilla.org/de/ | 2xx: 25 | � | 5607/10628 | 212 | nein | nein |  |  |
| 27 | https://docs.python.org/3/ | 2xx: 25, 3xx: 1 | ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: analyti | 3528/10628 | 212 | nein | nein |  |  |
| 28 | https://www.w3.org/ | 2xx: 28 | � | 5454/10628 | 211 | nein | nein |  |  |
| 29 | https://www.openstreetmap.org/ | 2xx: 13 | ESP_ERR_HTTP_CONNECT: Keine Verbindung ins Internet: matomo. | 7604/10628 | 211 | nein | nein |  |  |
| 30 | https://www.apple.com/ch-de/ | 2xx: 18, 4xx: 2 | � | 10372/10628 | 211 | nein | nein |  |  |

Neustarts der Firmware im Lauf (ohne den ersten): 0

JS-Fehler: 22
- 8� InternalError: out of memory
- 6� Error: chunk path empty but not in a worker
- 1� SyntaxError
- 1� TypeError: cannot read property 'parentElement' of undefined
- 1� TypeError: cannot read property 'document' of undefined
- 1� TypeError: cannot read property 'consent' of null
- 1� SyntaxError: redeclaration of 'prefersDark'
- 1� TypeError: not a function
- 1� TypeError: cannot read property 'prototype' of undefined
- 1� ReferenceError: ResizeObserver is not defined

CSS: nicht verstanden (je Seite die 25 h�ufigsten, summiert; 117 Arten)
| CSS | Deklarationen | Seiten |
|---|---|---|
| `var()` | 8677 | 18 |
| `-webkit-*` | 1267 | 19 |
| `calc()` | 1055 | 17 |
| `border-radius` | 889 | 19 |
| `transition` | 610 | 16 |
| `transform` | 547 | 19 |
| `box-shadow` | 527 | 16 |
| `gap` | 468 | 16 |
| `rgb()` | 468 | 3 |
| `fill` | 296 | 11 |
| `wert:display` | 258 | 8 |
| `pointer-events` | 250 | 14 |
| `grid-column` | 232 | 6 |
| `-moz-*` | 214 | 14 |
| `grid-template-columns` | 193 | 10 |
| `mask` | 174 | 1 |
| `background-size` | 164 | 12 |
| `-ms-*` | 159 | 6 |
| `mask-image` | 145 | 3 |
| `border-top-right-radius` | 142 | 6 |
| `border-top-left-radius` | 137 | 6 |
| `border-bottom-left-radius` | 128 | 4 |
| `font-variation-settings` | 126 | 1 |
| `animation` | 121 | 12 |
| `wert:background-color` | 116 | 3 |
| `margin-block-start` | 109 | 2 |
| `border-bottom-right-radius` | 107 | 3 |
| `grid-area` | 102 | 6 |
| `overflow-wrap` | 101 | 8 |
| `margin-inline-start` | 93 | 4 |
| `object-fit` | 93 | 6 |
| `linear-gradient()` | 91 | 8 |
| `transition-property` | 88 | 6 |
| `row-gap` | 88 | 5 |
| `inline-size` | 78 | 1 |
| `grid-row` | 72 | 4 |
| `aspect-ratio` | 69 | 6 |
| `padding-inline-start` | 67 | 3 |
| `inset-inline-start` | 66 | 3 |
| `filter` | 65 | 6 |
