# Using the hidden desktop: research

The wallpaper is almost always covered. This note collects academic and product prior art for two
surfaces glimpse could use instead:

- **A. The overview backdrop.** It is visible around the zoomed-out workspaces whenever niri's
  overview is open.
- **B. The Win-key dashboard.** An overlay of cards: applets, notes and tasks. Another session is
  shaping this one.

## Hard constraint from niri

- **The backdrop cannot be clicked.** The [niri wiki on layer rules](https://github.com/niri-wm/niri/wiki/Configuration:-Layer-Rules)
  says: "Layers within the backdrop will ignore all input". `place-within-backdrop` works only on
  background-layer surfaces that ignore exclusive zones.
- **Anything interactive has to be a top-layer surface.** It would be mapped on the IPC event
  `OverviewOpenedOrClosed { is_open }`. Top and overlay layers stay above the overview
  ([niri wiki, Overview](https://github.com/niri-wm/niri/wiki/Overview)). Whether they receive
  input there is untested.
- **Prior use is wallpaper only.** Noctalia and niri-wallpaper put a blurred copy of the wallpaper
  in the backdrop. No shell puts information there.

## What the research says

1. **Visible reminders bring work back.** In a field log of 27 users, windows that were less than
   25% visible were resumed significantly more slowly than windows more than 75% visible, and 27%
   of suspended tasks waited over 2 hours
   ([Iqbal & Horvitz, CHI 2007](https://www.erichorvitz.com/CHI_2007_Iqbal_Horvitz.pdf)).
2. **Show the relevant content, not a shrunken window.** Clipping Lists cut task time from 650 s
   to 616 s. A "changed" border alone gave no gain
   ([Matthews et al., CHI 2006](https://www.interruptions.net/literature/Matthews-CHI06-p989-matthews.pdf)).
3. **Your own things beat status.** People keep current work where they can see it, because seeing
   it reminds them ([Malone 1983](https://ir.webis.de/anthology/1983.tois_journal-ir0anthology0volumeA1A1.6),
   [Barreau & Nardi 1995](https://homepages.cwi.nl/~steven/sigchi/bulletin/1995.3/barreau.html)).
   HTC reported that fewer than 10% of users use widgets beyond clock, weather and music. That is an
   industry claim, and weak evidence.
4. **Keep positions fixed.** Spatial layouts were still recalled after months away
   ([Data Mountain](https://www.microsoft.com/en-us/research/wp-content/uploads/1999/01/interact99.pdf)).
   Fixed order beat recency order in window switchers
   ([Tak et al. 2009](https://research-portal.uu.nl/en/publications/improving-window-switching-interfaces/)).
5. **A glance lasts 2–5 s.** Half of smartwatch sessions are shorter than 5 s
   ([Visuri et al., CHI 2017](https://urn.fi/URN:NBN:fi-fe201801162071)). Glanceable displays use a
   few fixed states and symbols over text
   ([Matthews' glanceability work](https://www2.eecs.berkeley.edu/Pubs/TechRpts/2007/EECS-2007-56.html)).
6. **Ticket, then detail.** Sideshow showed one-line tickets that open into a fuller panel. It ran
   a 7-month field trial and was then adopted by thousands of Microsoft employees
   ([Cadiz et al.](https://www.interruptions.net/literature/Cadiz-tr-2001-83.pdf)).
7. **Notes are scraps.** They are terse, mixed and rarely revised. Capture must cost nothing: no
   title, no save step ([Bernstein et al., information scraps](https://hci.stanford.edu/publications/paper.php?id=236)).
   Many notes are temporary
   ([Lin et al., CHI 2004](https://doi.org/10.1145/985692.985779)).
8. **To-do lists stay simple.** People's own prioritising works; don't replace it with fields
   ([Bellotti et al., CHI 2004](https://hci.rwth-aachen.de/materials/conferences/CHI2004/1p735.pdf)).
9. **Batching notifications beats switching them off.** Three batches a day improved attention and
   mood. Turning notifications off raised anxiety
   ([Fitz et al. 2019](https://scholars.duke.edu/publication/1402953)). The overview is a natural
   batch point.
10. **Nothing recent covers this exact idea.** No 2018–2026 CHI, UIST or CSCW paper covers overview
    backdrops or desktop dashboards. The nearest work is about getting back into tasks:
    [ScreenTrack, CHI 2020](https://arxiv.org/pdf/2001.10898) and
    [SwitchBot, CHI 2018](https://www.microsoft.com/en-us/research/?p=465405).

## What failed, and why

| Product | Why it failed | Lesson |
| --- | --- | --- |
| [Apple Dashboard](https://www.techradar.com/news/dashboard-is-dumped-from-macos-catalina-and-some-folks-arent-happy) (2005–2019) | A separate widget world behind a key that few people opened | A key-only surface dies unless it holds the user's own state |
| [Windows 11 Widgets](https://www.neowin.net/news/top-10-features-and-changes-users-want-in-windows-11-widgets/) | The news feed. "Remove news" got 9,800+ upvotes | No feed. Show only what the user pinned |
| [Vista Gadgets](https://technet.microsoft.com/library/security/2719662) | Remote code execution through third-party gadgets | Cards only render state. User code runs out of process |
| [KDE Activities](https://pointieststick.com/2024/02/06/whats-going-on-with-activities-in-plasma-6), [Arc Spaces](https://www.theregister.com/2025/05/27/arc_browser_development_ends/) | Context systems stay niche | One dashboard, no modes |
| [Stage Manager](https://tidbits.com/2023/08/07/do-you-use-it-stage-manager-sees-weak-adoption/) | Used by 12% of respondents on Mac | Don't do window management there. niri already does |
| [Windows Recall](https://www.windowscentral.com/software-apps/windows-11/microsoft-addresses-windows-recall-backlash-promises-to-fix-security-issues-and-make-it-opt-in) | Screen history in a plaintext database | No screen memory in the shell |

## Ideas worth taking

### A. Overview backdrop (display only)

- **Info wallpaper.**
  - Contents: a big clock, today's agenda, weather, now playing, and the unread count with the
    latest summary.
  - It changes slowly, like the [TRMNL](https://the-gadgeteer.com/2025/07/21/trmnl-e-ink-dashboard-display-review-better-than-onscreen-widgets/)
    e-ink rule: "a glance, not an app".
  - It is tinted from the wallpaper palette, the way [Sonoma widgets](https://www.ghacks.net/?p=196956)
    fade when windows are open.
- **Left unfinished.** Windows that held focus for a while and were then buried by an alert, each
  with its one relevant line (Iqbal & Horvitz, Clipping Lists).
- **Workspace labels.** A name and a one-line note per workspace, so the overview reads as tasks
  ([Scalable Fabric](https://www.microsoft.com/en-us/research/publication/scalable-fabric-flexible-representation-task-management/),
  [Jeuris & Bardram 2016](https://pure.itu.dk/en/publications/dedicated-workspaces-faster-resumption-times-and-reduced-cognitiv/)).
- **Batched notifications.** Non-urgent notifications wait for the next time the overview opens
  (Fitz et al.).
- **One resurfaced scrap.** A note or reminder rotates in each time the overview opens
  ([Vocabulary Wallpaper](https://doi.org/10.1145/2207676.2208598), [Wait-Learning](https://sls.csail.mit.edu/publications/2015/Cai_CHI-2015.pdf)).
- **Glanceable wallpaper.** The live wallpaper effect follows the forecast, for example rain on
  rainy days ([InfoCanvas](https://sites.cc.gatech.edu/gvu/ii/infoart)).

### B. Win-key dashboard

- **Notes and tasks lead, status applets follow.** Research favours a dashboard of the user's own
  state. The rarely used applets fit as a "status" column, not the headline.
- **The notes card takes typing the moment the dashboard opens.** Plain text, append-only, and
  temporary notes can expire.
  - **Potluck-style.** The text recognises `[ ]` as a task and a date as an agenda entry
    ([Ink & Switch Potluck](https://www.inkandswitch.com/project/potluck/)).
- **The task list is short and ordered by hand, with inline add.** ChromeOS Glanceables could not
  add a task in place ([Chrome Unboxed](https://chromeunboxed.com/chromeos-google-tasks-shelf-widget)).
- **A dismissed notification can become a task with one action**
  ([Laevo, UIST 2014](https://doi.org/10.1145/2642918.2647391)).
- **Cards keep fixed positions.** Each card is a Sideshow ticket that opens into its detail.
- **Every card is addressable over IPC,** so a key can open a single card (the
  [DankMaterialShell](https://github.com/AvengeMedia/DankMaterialShell) pattern).
- **Type-to-search across the D-Bus providers**
  ([GNOME search providers](https://developer.gnome.org/documentation/tutorials/search-provider.html)).
- **Daily check-in.** The first open of the day shows "yesterday you were on…", built from the
  previous evening's notes and workspace names ([SwitchBot](https://www.microsoft.com/en-us/research/?p=465405)).
- **Tinkering path.** A user can drop a small exec applet into a card slot
  ([Ink & Switch, Malleable software](https://www.inkandswitch.com/essay/malleable-software/)).

## Further reading

- **Linux shells:**
  - [Caelestia](https://github.com/caelestia-dots/shell): a dashboard that also opens on hover.
  - [Noctalia](https://github.com/noctalia-dev/noctalia-shell): ships a backdrop surface.
  - [end-4](https://github.com/end-4/dots-hyprland): a "do" sidebar on one side, a "status"
    sidebar on the other.
  - [adi1090x/widgets](https://github.com/adi1090x/widgets): the typical r/unixporn card set.
  - [Glance](https://github.com/glanceapp/glance): a feed dashboard laid out in YAML.
- **Concepts:**
  - [Mercury OS](https://uxdesign.cc/introducing-mercury-os-f4de45a04289): flows of modules, close
    to niri's scrolling strip.
  - [Desktop Neo](https://desktopneo.com/): content cards with one or two actions; notifications
    count as read once seen.
  - [Obenauer's itemized OS](https://alexanderobenauer.com/labnotes/000/).
  - [Embark](https://www.inkandswitch.com/project/embark/).
  - [iOS StandBy](https://macstories.net/stories/ios-and-ipados-17-the-macstories-review/5): one big
    clock plus a rotating stack.
- **Classics:**
  - [Rooms](https://interlisp.org/history/bibliography/gwu6sw5w/).
  - [Task Gallery](https://www.microsoft.com/en-us/research/video/task-gallery-3d-window-manager/).
  - [GroupBar](https://www.microsoft.com/en-us/research/wp-content/uploads/2003/01/ozchi2003-groupbar.pdf).
  - [Lifestreams](https://sigmodrecord.org/publications/sigmodRecord/9603/pdfs/381854.381893.pdf).
  - [BumpTop](https://doi.org/10.1145/1124772.1124965).
  - [Pad++](https://www.cs.umd.edu/projects/hcil/jazz/learn/papers/uist-94-pad.pdf).
  - [Kimura](https://www.interruptions.net/literature/MacIntyre-UIST01-Support.pdf).
  - [Scope](https://www.microsoft.com/en-us/research/wp-content/uploads/2002/05/scope.pdf).
  - [Space to Think](https://doi.org/10.1145/1753326.1753336).
- **Evaluation:**
  - [McCrickard's IRC model](https://www.interruptions.net/literature/McCrickard-TOCHI03.pdf):
    rates interruption, reaction and comprehension.
  - [Mankoff's ambient heuristics](https://depts.washington.edu/chilllab/?p=204): for reviewing
    previews.
  - [Pousman & Stasko's taxonomy](https://faculty.cc.gatech.edu/~stasko/papers/avi06.pdf).

Some figures could not be checked against the original paper. Treat them as unverified:

- the 3.8 s Apple Watch glance
- list.it's "half of notes in under 10 s"
- Gloria Mark's 47 s on one screen
