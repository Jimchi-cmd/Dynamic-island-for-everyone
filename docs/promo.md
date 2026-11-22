# Promo videos

Ten short vertical videos (1080×1920, 13–17 s) for TikTok, Instagram Reels and YouTube Shorts, and a one-minute install guide (1920×1080) for YouTube. The island in each one is drawn by Islet's own code, frame by frame, with its real animation, so what people see is what they get.

## Making them

```bash
cd android && ISLET_PROMO=1 gradle :app:testDebugUnitTest --tests '*PromoFrames*' --rerun
```

```bash
ISLET_PROMO_DIR=build/promo-laptop cargo test -p islet --bin islet promo_frames
```

```bash
python3 scripts/promo/make-videos.py --out promo-videos
```

The first two draw the island's frames (the phone's and the laptop's); the last lays them over a phone or laptop screen with captions and encodes the videos (it needs Pillow and ffmpeg). The scripts of what happens when are in `PromoFrames.kt`, `promo_frames` in `crates/islet/src/app.rs`, and the captions in `scripts/promo/make-videos.py`.

The videos are silent on purpose. Add a sound when you post, from TikTok's, Instagram's or YouTube's own library: those are licensed for use on that app, and trending sounds help a video get shown.

## Launch reels (the newest style)

Five 29-second vertical reels in the style of an app-launch video: the island floating big on a deep, lit background (no fake phone), a 3D title, words that fade in from a blur, a punch-in on every bar and a zoom-blur between parts, an "And also…" stack, an "All free" card and a Download on GitHub ending. The music is an original 170 BPM track (breakbeat, bass, chord stabs and a lead hook) made by `music.banger()`, with a snare roll into the first drop and a crash on every new part.

```bash
python3 scripts/promo/make-reels.py --out promo-videos
```

### Cut to a song of your choice

```bash
python3 scripts/promo/make-reels.py --out promo-videos --song your-song.mp3
```

`beats.py` finds the song's tempo, beats and bars; each reel starts two bars before a drop (set per reel with `song_bar`), every cut lands on a bar line, the island's moments are nudged onto beats, and the punches, light streaks and glow follow the song's kicks, snares and loudness. Each reel is written twice: with the song, and silent (`-silent.mp4`).

**Copyright:** a commercial song inside the video file is often muted or claimed. Post the silent version and pick the same song from the app's own sound library (TikTok and Instagram license their libraries); the timing still matches if the in-app sound starts at the same point.

## "Islet in real life" (with music)

Five more vertical videos, 25–26 s each, with an original soundtrack and smooth camera moves: Islet in a meeting, in class, on the way, at the gym and at your desk, then an Islet Pro card with its extras and the GitHub link.

```bash
python3 scripts/promo/make-life-videos.py --out promo-videos
```

The music is made from scratch by `scripts/promo/music.py` (a kick, claps, hi-hats, bass, chords and a melody, all synthesised), so it's yours to post anywhere; each video gets its own key and tempo, with a riser that lands on the Pro card.

**Don't post the Pro ending:** Islet Pro is gone and everything is free, so that card is out of date. Post the ten earlier videos, or trim the last few seconds.

## The full install guide (YouTube)

`scripts/promo/make-tutorial.py` makes a 3-minute, 1920×1080 guide in chapters: the phone (download, allowing the install, notification access, the accessibility switch and the "restricted settings" fix), Windows (SmartScreen), Mac (Open Anyway), Linux, and pairing the phone with the computer. Each step shows the screen with the exact button highlighted and a finger that taps it. It also writes the chapter list to paste into the YouTube description.

```bash
python3 scripts/promo/make-tutorial.py --out promo-videos --laptop-frames build/promo-laptop-desk
```

## Covers, profile picture and YouTube thumbnail

The old Islet Pro cover (2560 x 1440) and square thumbnail (1200 x 1200) were for a store page that is gone. The same script also makes a profile picture (`--profile`) and the tutorial's YouTube thumbnail (`--youtube`). They are drawn from an island frame (the open music card looks best):

```bash
ISLET_PROMO_DIR=/tmp/islet-desk ISLET_PROMO_REEL=desk cargo test -p islet --bin islet promo_frames
```

```bash
python3 scripts/promo/make-cover.py /tmp/islet-desk/f0400.png /tmp/islet-cover
```

## Posting

- **Post one a day** rather than all at once, and pin the best one to your profile.
- **Put the download link in your bio:** `https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/releases/latest`. TikTok only allows a bio link once you have a business account or enough followers; until then, say "search Islet on GitHub" in the caption.
- **Answer comments** with a short reply or a reply video; it keeps a video going.
- **Say what it is honestly.** Islet is inspired by the iPhone's Dynamic Island, but it isn't made by or affiliated with Apple. Calling it "Dynamic Island-style" is fine; don't use Apple's logo, screenshots or sounds.

## Captions

Each is a starting point. Hashtags go at the end.

1. **islet-01-music.mp4**: Your camera hole can show what's playing now 🎵 Cover, controls and live lyrics, for any music app. Free, link in bio. #android #androidtips #dynamicisland #music #techtok
2. **islet-02-calls.mp4**: Answer calls right from the top of your screen 📞 Free app for Android. #android #androidtricks #dynamicisland #techtok #phonehacks
3. **islet-03-notifications.mp4**: Notifications that pop out of your camera, with Reply right there. #android #androidtips #customization #dynamicisland #techtok
4. **islet-04-tap-the-island.mp4**: Tap your camera. Once for battery and time, twice for your day 👀 #android #androidhacks #dynamicisland #productivity #techtok
5. **islet-05-control-centre.mp4**: Hold the island for your own control centre. #android #androidtips #customization #techtok #phonetips
6. **islet-06-students.mp4**: Class countdowns and deadlines on your phone's camera 📚 Made for students. #studytok #student #android #productivity #dynamicisland
7. **islet-07-directions.mp4**: Never miss a turn: directions on the island, music right next to it. #android #maps #dynamicisland #techtok #roadtrip
8. **islet-08-focus-streak.mp4**: Focus rounds with a streak, right on your camera 🔥 #studytok #productivity #pomodoro #android #focus
9. **islet-09-quick-tour.mp4**: One island. Everything. Free and open source. #android #dynamicisland #opensource #techtok #androidtips
10. **islet-10-laptop.mp4**: It's on your laptop too: lyrics, battery, weather and more at the top of your screen 💻 Windows, Mac and Linux. #windows11 #macbook #setup #desksetup #productivity

**YouTube (install guide), title:** How to get a Dynamic Island-style island on Android & PC (free, 1 minute)

**Description:** Islet puts an island round your phone's camera and at the top of your laptop's screen: music with lyrics, calls, notifications, timers, your calendar and more. Free and open source. Download: https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/releases/latest. Not made by or affiliated with Apple.
