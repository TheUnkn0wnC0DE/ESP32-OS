"""BR1/BR2: Board-Log eines Seitenlisten-Laufs (br_lauf.ps1) auswerten.

Je Seite: Abrufe (HTTP-Status), Fehlermeldungen des Abrufers, Browser-Heap
(Spitze), interner RAM danach, ob der Browser vorzeitig endete, Abstürze
der Firmware. Ausgabe: Markdown-Tabelle; die Spalten „Ergebnis“ und
„Grund“ werden nach Sicht der Fotos (bild_<i>.png) ausgefüllt. Darunter
(Q3) JS-Fehler nach Häufigkeit und die nicht verstandenen CSS-Funktionen
(Zeile „ns_css“, letzte je Seite) summiert über alle Seiten.

    python -I br_auswertung.py ZIELORDNER/board_log.txt > tabelle.md
"""
import re
import sys
from collections import Counter

log = open(sys.argv[1], encoding='utf-8', errors='replace').read().splitlines()
seiten = {}
aktuell = None
neustarts = 0
js_fehler = Counter()
for z in log:
    m = re.search(r'SEITE (\d+)/(\d+): (https?://\S+)', z)
    if m:
        aktuell = int(m.group(1))
        seiten[aktuell] = {'url': m.group(3), 'status': Counter(), 'fehler': [], 'spitze': '', 'intern': '',
                           'vorzeitig': False, 'absturz': [], 'css': ''}
        continue
    if re.search(r'^rst:0x', z):
        neustarts += 1
    if aktuell is None:
        continue
    s = seiten[aktuell]
    m = re.search(r'ns_abruf: (-?\d+), \d+ Byte', z)
    if m:
        s['status'][m.group(1)[0] + 'xx' if len(m.group(1)) == 3 else m.group(1)] += 1
    if re.search(r'[WE] \(\d+\) ns_abruf', z) or 'Zu wenig Speicher' in z:
        s['fehler'].append(z.split('ns_abruf:')[-1].strip()[:60])
    m = re.search(r'Spitze (\d+) KB von (\d+) KB', z)
    if m:
        s['spitze'] = f'{m.group(1)}/{m.group(2)}'
    m = re.search(r'danach (\d+) KB interner RAM', z)
    if m:
        s['intern'] = m.group(1)
    m = re.search(r'ns_js: Fehler: (.*)$', z)
    if m:
        js_fehler[m.group(1).strip()[:90]] += 1
    m = re.search(r'ns_css: Lücken \(\d+ Arten, \d+ Meldungen\): (.*)$', z)
    if m:
        s['css'] = m.group(1)
    if 'Browser läuft nicht mehr' in z:
        s['vorzeitig'] = True
    for k in ('Guru Meditation', 'abort() was called', 'PANIK', 'Absturz von'):
        if k in z:
            s['absturz'].append(k)

print('| # | Seite | Abrufe (Status) | Abruf-Fehler | Heap Spitze/Grösse KB | intern danach KB | Browser vorzeitig weg | Absturz | Ergebnis | Grund |')
print('|---|---|---|---|---|---|---|---|---|---|')
for i in sorted(seiten):
    s = seiten[i]
    st = ', '.join(f'{k}: {v}' for k, v in sorted(s['status'].items())) or '–'
    fe = '; '.join(sorted(set(s['fehler']))[:3]) or '–'
    print(f"| {i} | {s['url']} | {st} | {fe} | {s['spitze'] or '–'} | {s['intern'] or '–'} | "
          f"{'ja' if s['vorzeitig'] else 'nein'} | {', '.join(s['absturz']) or 'nein'} |  |  |")
print()
print(f'Neustarts der Firmware im Lauf (ohne den ersten): {max(0, neustarts - 1)}')

print()
print(f'JS-Fehler: {sum(js_fehler.values())}')
for t, n in js_fehler.most_common():
    print(f'- {n}× {t}')
css_summe = Counter()
css_seiten = Counter()
for s in seiten.values():
    teile = s['css'].split(' (+')[0]
    for e in filter(None, teile.split(', ')):
        name, _, n = e.rpartition(' ')
        if n.isdigit():
            css_summe[name] += int(n)
            css_seiten[name] += 1
print()
print(f'CSS: nicht verstanden (je Seite die 25 häufigsten, summiert; {len(css_summe)} Arten)')
print('| CSS | Deklarationen | Seiten |')
print('|---|---|---|')
for name, n in css_summe.most_common(40):
    print(f'| `{name}` | {n} | {css_seiten[name]} |')
