/*
Copyright (C) 1996-1997 Id Software, Inc.

This program is free software; you can redistribute it and/or
modify it under the terms of the GNU General Public License
as published by the Free Software Foundation; either version 2
of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  

See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program; if not, write to the Free Software
Foundation, Inc., 59 Temple Place - Suite 330, Boston, MA  02111-1307, USA.

*/
// sys_null.h -- null system driver to aid porting efforts

#include "quakedef.h"
#include "errno.h"

qboolean isDedicated;

/*
===============================================================================

FILE IO

===============================================================================
*/

#define MAX_HANDLES             10
FILE    *sys_handles[MAX_HANDLES];

// Konsole: die Spieldaten (id1/pak0.pak) liefert die Rust-Seite (Flash-
// Partition bzw. Speicher); Griffe mit eigener Leseposition.
extern int QG_DateiGroesse(const char *pfad);
extern int QG_DateiLesen(int pos, void *ziel, int anzahl);
extern double QG_Zeit(void);
extern void QG_Beenden(void);
extern void QG_Fehler(const char *meldung);
static int qg_pak_pos[MAX_HANDLES];
static int qg_pak_auf[MAX_HANDLES];

int             findhandle (void)
{
	int             i;
	
	for (i=1 ; i<MAX_HANDLES ; i++)
		if (!sys_handles[i] && !qg_pak_auf[i])
			return i;
	Sys_Error ("out of handles");
	return -1;
}

/*
================
filelength
================
*/
int filelength (FILE *f)
{
	int             pos;
	int             end;

	pos = ftell (f);
	fseek (f, 0, SEEK_END);
	end = ftell (f);
	fseek (f, pos, SEEK_SET);

	return end;
}

int Sys_FileOpenRead (char *path, int *hndl)
{
	FILE    *f;
	int             i;
	
	i = findhandle ();

	{
		int groesse = QG_DateiGroesse(path);
		if (groesse >= 0)
		{
			for (i = 1; i < MAX_HANDLES; i++)
				if (!sys_handles[i] && !qg_pak_auf[i])
					break;
			if (i == MAX_HANDLES)
				Sys_Error ("out of handles");
			qg_pak_auf[i] = 1;
			qg_pak_pos[i] = 0;
			*hndl = i;
			return groesse;
		}
	}

	f = fopen(path, "rb");
	if (!f)
	{
		*hndl = -1;
		return -1;
	}
	sys_handles[i] = f;
	*hndl = i;
	
	return filelength(f);
}

int Sys_FileOpenWrite (char *path)
{
	FILE    *f;
	int             i;
	
	i = findhandle ();

	f = fopen(path, "wb");
	if (!f)
		Sys_Error ("Error opening %s: %s", path,strerror(errno));
	sys_handles[i] = f;
	
	return i;
}

void Sys_FileClose (int handle)
{
	if (qg_pak_auf[handle])
	{
		qg_pak_auf[handle] = 0;
		return;
	}
	fclose (sys_handles[handle]);
	sys_handles[handle] = NULL;
}

void Sys_FileSeek (int handle, int position)
{
	if (qg_pak_auf[handle])
	{
		qg_pak_pos[handle] = position;
		return;
	}
	fseek (sys_handles[handle], position, SEEK_SET);
}

int Sys_FileRead (int handle, void *dest, int count)
{
	if (qg_pak_auf[handle])
	{
		int n = QG_DateiLesen(qg_pak_pos[handle], dest, count);
		if (n > 0)
			qg_pak_pos[handle] += n;
		return n;
	}
	return fread (dest, 1, count, sys_handles[handle]);
}

int Sys_FileWrite (int handle, void *data, int count)
{
	return fwrite (data, 1, count, sys_handles[handle]);
}

int     Sys_FileTime (char *path)
{
	FILE    *f;
	
	if (QG_DateiGroesse(path) >= 0)
		return 1;
	f = fopen(path, "rb");
	if (f)
	{
		fclose(f);
		return 1;
	}
	
	return -1;
}

void Sys_mkdir (char *path)
{
}


/*
===============================================================================

SYSTEM IO

===============================================================================
*/

void Sys_MakeCodeWriteable (unsigned long startaddr, unsigned long length)
{
}


void Sys_Error (char *error, ...)
{
	va_list         argptr;
	static char     text[1024];

	va_start (argptr,error);
	vsnprintf (text,sizeof(text),error,argptr);
	va_end (argptr);

	// Konsole: Meldung an die Rust-Seite (Haken quake::beim_fehler, in der
	// Firmware: Log, Meldung sichern, Neustart); QG_Fehler kehrt nicht zurück.
	QG_Fehler (text);
}

void Sys_Printf (char *fmt, ...)
{
	va_list         argptr;
	
	va_start (argptr,fmt);
	vprintf (fmt,argptr);
	va_end (argptr);
}

void Sys_Quit (void)
{
	// Konsole: zurück ins Menü der Konsole (Neustart bzw. Ende am PC).
	QG_Beenden();
	exit (0);
}

double Sys_FloatTime (void)
{
	return QG_Zeit();
}

char *Sys_ConsoleInput (void)
{
	return NULL;
}

void Sys_Sleep (void)
{
}

void Sys_SendKeyEvents (void)
{
}

void Sys_HighFPPrecision (void)
{
}

void Sys_LowFPPrecision (void)
{
}

//=============================================================================

/*

void main (int argc, char **argv)
{
	static quakeparms_t    parms;

	parms.memsize = 8*1024*1024;
	parms.membase = malloc (parms.memsize);
	parms.basedir = ".";

	COM_InitArgv (argc, argv);

	parms.argc = com_argc;
	parms.argv = com_argv;

	printf ("Host_Init\n");
	Host_Init (&parms);
	while (1)
	{
		Host_Frame (0.1);
	}
}

*/
