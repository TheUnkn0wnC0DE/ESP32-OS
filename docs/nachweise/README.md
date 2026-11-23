# Evidence ("Nachweise")

Screenshots and serial logs recorded on the board, one set per work item in
[`../development/PLAN.md`](../development/PLAN.md). File names start with the
item ID (for example `o4_…` for the acceptance run, `z1_…` for the PDF viewer);
newer items have their own folder. Screenshots come from the framebuffer via
`esp32/firmware/bildschirmfoto.py`; logs are checked for Wi-Fi credentials
with `esp32/firmware/werkzeuge/log_schwaerzen.py`.
