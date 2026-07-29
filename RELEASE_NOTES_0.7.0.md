# Spotiamp+ 0.7.0

The big one: **play your own local files** — plus EQ presets you can save, a
date-added column in the library, and windows that come back where you left them.

## New

- **Local file playback.** Play your own MP3, FLAC, M4A/AAC, WAV, OGG or Opus
  files right alongside Spotify.
  - Drag files (or a whole selection) from Explorer onto the player.
  - Or, from the playlist menu (right-click → **List**), pick **Add local
    file(s)…** / **Add local folder…** (a folder plays through, subfolders and all).
  - Or press **O** for a file picker, **Shift+O** for a folder.
  - Track names, artist and length come from the file's own tags; the EQ and
    visualizer work on local files just like on Spotify.
- **Save your equaliser.** The EQ could load `.EQF` presets — now it can save
  them too (presets menu → **Save .EQF…**), so a curve you like is yours to keep
  (and it's a real Winamp preset, usable in Winamp itself).
- **Library “Date” column** showing when each track was added to the playlist,
  click-sortable like the other columns.
- **Windows reopen on launch.** If you had the library, visualiser or lyrics
  window open when you quit, they come back — same place, same size.

## Improved

- The library, visualiser and lyrics windows have a cleaner frame that matches
  the skin's own colours instead of a fixed edge.
- The library's bottom bar (Play / Enqueue / Play all / +List) now matches every
  skin, including ones without a GENEX button sheet.
- The visualiser's window remembers its size, and **Pin** holds the current
  pattern instead of cycling.

## Fixed

- **Shuffle + “autoplay similar” now work together** — a shuffled queue reaches
  its end properly, so Spotify-radio autoplay kicks in (it used to shuffle
  forever and never trigger).

## Known issues

- **Date is blank for Liked Songs and search results** — a track's added-date
  only comes through for playlists; the liked/search list doesn't carry it yet.
- **Local files play as their own queue**, not yet as rows in the playlist
  window (drag-drop / picker start them; next/previous walk the local set).
