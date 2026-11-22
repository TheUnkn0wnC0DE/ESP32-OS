/* ESP32-OS-Test für proc_lib (Datei- und Farbdialog), inhaltlich wie das
 * KolibriOS-Beispiel samples/clayer/dialog.c, aber gültiges Standard-C
 * (das Original castet einen uint auf rgb_t – nur mit tcc übersetzbar).
 * Ergebnis zusätzlich in /sys/dialog_ergebnis.txt für die Prüfung. */
#include <clayer/dialog.h>
#include <stdio.h>
#include <sys/ksys.h>

int main()
{
    open_dialog* dlg_open = kolibri_new_open_dialog(OPEN, 10, 10, 420, 320);
    OpenDialog_init(dlg_open);
    OpenDialog_start(dlg_open);

    color_dialog* color_select = kolibri_new_color_dialog(SELECT, 10, 10, 420, 320);
    ColorDialog_init(color_select);
    ColorDialog_start(color_select);

    FILE* f = fopen("/sys/dialog_ergebnis.txt", "w");
    if (dlg_open->status == SUCCESS) {
        printf("File selected '%s'\n", dlg_open->openfile_path);
        if (f) fprintf(f, "datei=%s\n", dlg_open->openfile_path);
    } else {
        puts("No file selected!");
        if (f) fprintf(f, "datei=-\n");
    }
    if (color_select->status == SUCCESS) {
        rgb_t c;
        c.red = (color_select->color >> 16) & 255;
        c.green = (color_select->color >> 8) & 255;
        c.blue = color_select->color & 255;
        printf("Color selected: #%06X\n", color_select->color);
        printf("Red:%d Green:%d Blue:%d\n", c.red, c.green, c.blue);
        if (f) fprintf(f, "farbe=%06X\n", color_select->color);
    } else {
        puts("No color selected!");
        if (f) fprintf(f, "farbe=-\n");
    }
    if (f) fclose(f);
    free(dlg_open);
    free(color_select);
    return 0;
}
