
use crate::menu::{Menu, Piece, Rect, Style, kind, piece, placed};
use crate::options_screen::{
    self, AUDIO, Bytes, COLOUR, FIRST, OptionsScreen, SUBTITLES, Settings, VIBRATION,
};
use crate::player_panel::{Drawn, text_at};
use crate::rooms::Image;

/// The lists of lines in the screen's file: the first list; the labels of the page of
/// the sound, the answers of its two volumes and of the output format; the labels of the
/// page of the texts and effects and the answers of its seven rows; the label and the
/// answers of the vibration.
pub const FIRST_LIST: u32 = 0x800C_4FE4;
pub const AUDIO_LABELS: u32 = 0x800C_50A0;
pub const AUDIO_ANSWERS: [u32; 3] = [0x800C_54A0, 0x800C_54C4, 0x800C_54E8];
pub const SUBTITLE_LABELS: u32 = 0x800C_567C;
pub const SUBTITLE_ANSWERS: [u32; 7] = [
    0x800C_577C,
    0x800C_5800,
    0x800C_587C,
    0x800C_59F0,
    0x800C_5A6C,
    0x800C_58F8,
    0x800C_5974,
];
pub const VIBRATION_LABEL: u32 = 0x800C_5AB0;
pub const VIBRATION_ANSWERS: u32 = 0x800C_5B28;

/// The heading's frame on the first list and on a page, and its word with its place
/// for each of the five.
pub const HEADING: [Rect; 2] = [
    Rect {
        x: 125,
        y: 18,
        w: 70,
        h: 18,
    },
    Rect {
        x: 114,
        y: 18,
        w: 92,
        h: 18,
    },
];
pub const HEADINGS: [(usize, (i16, i16)); 5] = [
    (79, (132, 21)),
    (221, (139, 21)),
    (232, (120, 21)),
    (42, (140, 20)),
    (171, (133, 21)),
];

/// The page of the colours: the four are 38 apart from the first; for each the small
/// frame that pulses under the cursor, the game's logo and the frame round it.
pub const COLOURS: i16 = 4;
pub const COLOUR_STEP: i16 = 38;
pub const COLOUR_MARK: Rect = Rect {
    x: 82,
    y: 42,
    w: 18,
    h: 36,
};
pub const COLOUR_BOX: Rect = Rect {
    x: 102,
    y: 42,
    w: 118,
    h: 36,
};
pub const LOGO: usize = 202;
pub const LOGO_AT: (i16, i16) = (116, 44);

/// The box of the line of help with the place of its text, and the box of a refusal
/// with the place of its first line; a refusal's lines are 12 apart (HYPOTHESIS: the
/// step of a message's lines).
pub const HELP_BOX: Rect = Rect {
    x: 10,
    y: 194,
    w: 300,
    h: 18,
};
pub const HELP_TEXT: (i16, i16) = (14, 197);
pub const REFUSAL_BOX: Rect = Rect {
    x: 10,
    y: 194,
    w: 300,
    h: 34,
};
pub const REFUSAL_TEXT: (i16, i16) = (13, 196);
pub const REFUSAL_STEP: i16 = 12;

/// The table of the tables of the line of help, one for each page, and how many texts
/// each holds; the two lines of the two refusals; the legend's list.
pub const HELP_TABLES: u32 = 0x800C_4EC8;
pub const HELP_COUNTS: [usize; 5] = [4, 3, 7, 1, 1];
pub const REFUSALS: [[u32; 2]; 2] = [[0x800C_2FDC, 0x800C_3044], [0x800C_2F28, 0x800C_2F8C]];
pub const LEGEND: u32 = 0x800C_5B34;

/// The flag word of a list whose cursor's line pulses, and of one whose cursor's line
/// has the marked colour.
const PULSES: u32 = 0x02;
const MARKED: u32 = 0x04;

/// The lists of lines of the screen's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lines {
    pub first: Vec<Vec<Piece>>,
    pub audio_labels: Vec<Vec<Piece>>,
    pub audio_answers: [Vec<Vec<Piece>>; 3],
    pub subtitle_labels: Vec<Vec<Piece>>,
    pub subtitle_answers: [Vec<Vec<Piece>>; 7],
    pub vibration_label: Vec<Vec<Piece>>,
    pub vibration_answers: Vec<Vec<Piece>>,
}

impl Lines {
    /// Reads the lists from the screen's file, whose first byte is at `base`.
    pub fn from_file(file: &[u8], base: u32) -> Option<Self> {
        let at = |list: u32| Menu::lines_at(file, base, list);
        let several = |lists: &[u32]| {
            lists
                .iter()
                .map(|&list| at(list))
                .collect::<Option<Vec<_>>>()
        };
        Some(Self {
            first: at(FIRST_LIST)?,
            audio_labels: at(AUDIO_LABELS)?,
            audio_answers: several(&AUDIO_ANSWERS)?.try_into().ok()?,
            subtitle_labels: at(SUBTITLE_LABELS)?,
            subtitle_answers: several(&SUBTITLE_ANSWERS)?.try_into().ok()?,
            vibration_label: at(VIBRATION_LABEL)?,
            vibration_answers: at(VIBRATION_ANSWERS)?,
        })
    }
}

/// The texts of the screen's file: the line of help of every line of every page, and
/// the two lines of each refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texts {
    pub help: [Vec<Vec<u16>>; 5],
    pub refusals: [[Vec<u16>; 2]; 2],
}

impl Texts {
    /// Reads the texts from the screen's file, whose first byte is at `base`.
    pub fn from_file(file: &[u8], base: u32) -> Option<Self> {
        let image = Image { data: file, base };
        let mut help: [Vec<Vec<u16>>; 5] = Default::default();
        for (page, (texts, count)) in help.iter_mut().zip(HELP_COUNTS).enumerate() {
            let table = image.u32(HELP_TABLES + 4 * page as u32).ok()?;
            for line in 0..count {
                let address = image.u32(crate::rooms::step(table, line, 4).ok()?).ok()?;
                texts.push(text_at(file, base, address)?);
            }
        }
        let line = |address: u32| text_at(file, base, address);
        let mut refusals: [[Vec<u16>; 2]; 2] = Default::default();
        for (texts, addresses) in refusals.iter_mut().zip(REFUSALS) {
            *texts = [line(addresses[0])?, line(addresses[1])?];
        }
        Some(Self { help, refusals })
    }
}

/// A list through the shared menu drawer: every line's frame in the style of the
/// list's flag word and its picture in its room, at half its brightness for a line
/// that is dim.
fn list(
    lines: &[Vec<Piece>],
    flags: u32,
    cursor: i16,
    time: u32,
    width: &impl Fn(usize) -> i16,
    dim: impl Fn(usize) -> bool,
) -> Vec<Drawn> {
    let mut drawn = Vec::new();
    for (n, pieces) in lines.iter().enumerate() {
        let (style, pulses) = Style::of_line(flags, usize::try_from(cursor) == Ok(n), 0);
        for piece in pieces {
            let clip = piece.rect;
            if piece.flags & piece::BARE == 0 {
                drawn.push(Drawn::Frame {
                    rect: clip,
                    mode: style.mode,
                    time: if pulses { time } else { 0 },
                });
            }
            let shows = piece.flags & piece::EMPTY == 0 && piece.kind == kind::PICTURE;
            if !shows || piece.value == u32::MAX {
                continue;
            }
            let number = piece.value as usize;
            let at = placed(piece.flags, clip.inside(piece.margin), width(number));
            drawn.push(match dim(n) {
                true => Drawn::DimSprite { number, at, clip },
                false => Drawn::Sprite { number, at, clip },
            });
        }
    }
    drawn
}

/// The labels of a page and the answers of each of its rows. The cursor's label pulses
/// and every row's value has the marked colour; while an answer is picked it is the
/// other way round for that row.
fn page(
    screen: &OptionsScreen,
    labels: &[Vec<Piece>],
    answers: &[Vec<Vec<Piece>>],
    values: &[i16],
    time: u32,
    width: &impl Fn(usize) -> i16,
    dim: impl Fn(usize) -> bool,
) -> Vec<Drawn> {
    let row = screen.cursor.at;
    let of_labels = if screen.picking { MARKED } else { PULSES };
    let mut drawn = list(labels, of_labels, row, time, width, dim);
    for (n, (lines, &value)) in answers.iter().zip(values).enumerate() {
        let picked = screen.picking && usize::try_from(row) == Ok(n);
        let flags = if picked { PULSES } else { MARKED };
        drawn.extend(list(lines, flags, value, time, width, |_| false));
    }
    drawn
}

/// The page of the colours: for each colour the frame round the logo, the logo, and
/// the small frame left of it, which pulses under the cursor. (HYPOTHESIS: in this
/// order from underneath, the reverse of the order they are added in.)
fn colours(screen: &OptionsScreen, time: u32) -> Vec<Drawn> {
    let whole = Rect {
        x: 0,
        y: 0,
        w: 320,
        h: 240,
    };
    let mut drawn = Vec::new();
    for colour in 0..COLOURS {
        let down = COLOUR_STEP * colour;
        let lower = |rect: Rect| Rect {
            y: rect.y + down,
            ..rect
        };
        drawn.push(Drawn::FrameOf {
            rect: lower(COLOUR_BOX),
            colour,
            mode: 0,
            time: 0,
        });
        drawn.push(Drawn::Sprite {
            number: LOGO,
            at: (LOGO_AT.0, LOGO_AT.1 + down),
            clip: whole,
        });
        drawn.push(Drawn::FrameOf {
            rect: lower(COLOUR_MARK),
            colour,
            mode: 1,
            time: if screen.cursor.at == colour { time } else { 0 },
        });
    }
    drawn
}

/// What the screen draws in a pass (`0x800C3158`, `0x800C47F0` while an answer is
/// picked and `0x800C4D3C` while a refusal is up), from what lies underneath to what
/// lies on top: the heading, the page, and the line of help or the box of a refusal.
/// `settings` are the game's, for the two volumes and for the rows that are dim;
/// `time` is the menus' time and `width` gives the width of a sprite of the pool.
pub fn drawn(
    screen: &OptionsScreen,
    settings: &Settings,
    lines: &Lines,
    time: u32,
    width: impl Fn(usize) -> i16,
) -> Vec<Drawn> {
    let whole = Rect {
        x: 0,
        y: 0,
        w: 320,
        h: 240,
    };
    let page_number = usize::try_from(screen.page).unwrap_or(0).min(4);
    let (word, at) = HEADINGS[page_number];
    let mut drawn = vec![
        Drawn::Frame {
            rect: HEADING[usize::from(screen.page != FIRST)],
            mode: 0,
            time: 0,
        },
        Drawn::Sprite {
            number: word,
            at,
            clip: whole,
        },
    ];
    let bytes = screen.bytes;
    match screen.page {
        FIRST => drawn.extend(list(
            &lines.first,
            PULSES,
            screen.cursor.at,
            time,
            &width,
            |_| false,
        )),
        AUDIO => {
            let values = [
                settings.music().into(),
                settings.effects().into(),
                bytes.output.into(),
            ];
            drawn.extend(page(
                screen,
                &lines.audio_labels,
                &lines.audio_answers,
                &values,
                time,
                &width,
                |_| false,
            ));
        }
        SUBTITLES => {
            let values = [
                bytes.display.into(),
                i16::from(bytes.anim_cut != [0, 0]),
                bytes.speech.into(),
                bytes.auto_play.into(),
                bytes.subtitles.into(),
                bytes.magic_fx.into(),
                bytes.capture_fx.into(),
            ];
            // The dim labels are set while no answer is picked and stay as they are
            // while one is, when the bytes are still what the settings gave.
            let set = if screen.picking {
                Bytes::read(settings)
            } else {
                bytes
            };
            let dim = |row: usize| match row as i16 {
                options_screen::SPEECH => set.subtitles == 0,
                options_screen::AUTO_PLAY => set.subtitles == 0 || set.speech == 0,
                _ => false,
            };
            drawn.extend(page(
                screen,
                &lines.subtitle_labels,
                &lines.subtitle_answers,
                &values,
                time,
                &width,
                dim,
            ));
        }
        COLOUR => drawn.extend(colours(screen, time)),
        VIBRATION => drawn.extend(page(
            screen,
            &lines.vibration_label,
            std::slice::from_ref(&lines.vibration_answers),
            &[bytes.vibration.into()],
            time,
            &width,
            |_| false,
        )),
        _ => {}
    }
    if screen.refusal != 0 {
        drawn.push(Drawn::Frame {
            rect: REFUSAL_BOX,
            mode: 0,
            time: 0,
        });
        let which = usize::from(screen.refusal - 1);
        drawn.extend((0..2).map(|line| Drawn::OptionsRefused {
            which,
            line,
            at: (REFUSAL_TEXT.0, REFUSAL_TEXT.1 + REFUSAL_STEP * line as i16),
        }));
    } else {
        drawn.push(Drawn::Frame {
            rect: HELP_BOX,
            mode: 0,
            time: 0,
        });
        let line = match screen.page {
            COLOUR | VIBRATION => 0,
            _ => usize::try_from(screen.cursor.at).unwrap_or(0),
        };
        drawn.push(Drawn::OptionsHelp {
            page: page_number,
            line,
            at: HELP_TEXT,
        });
    }
    drawn
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::{self, Pad, Repeat};

    fn line(sprite: u32, x: i16, y: i16, w: i16) -> Vec<Piece> {
        vec![Piece {
            flags: piece::CENTRED,
            kind: kind::PICTURE,
            value: sprite,
            rect: Rect { x, y, w, h: 18 },
            margin: (0, 3),
        }]
    }

    fn lines() -> Lines {
        let pair = |y: i16| vec![line(1, 125, y, 75), line(2, 201, y, 75)];
        Lines {
            first: (0..4)
                .map(|n| line(221 + n, 114, 81 + 20 * n as i16, 92))
                .collect(),
            audio_labels: (0..3).map(|n| line(126, 36, 83 + 20 * n, 79)).collect(),
            audio_answers: [
                (0..8).map(|n| line(133, 118 + 19 * n, 83, 18)).collect(),
                (0..8).map(|n| line(133, 118 + 19 * n, 103, 18)).collect(),
                pair(123),
            ],
            subtitle_labels: (0..7).map(|n| line(200, 40, 41 + 20 * n, 79)).collect(),
            subtitle_answers: std::array::from_fn(|n| pair(41 + 20 * n as i16)),
            vibration_label: vec![line(171, 40, 108, 79)],
            vibration_answers: pair(108),
        }
    }

    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(buttons, Repeat::of(1), 1);
        pad
    }

    /// The frames of what is drawn, as (x, y, mode, pulses).
    fn frames(drawn: &[Drawn]) -> Vec<(i16, i16, u8, bool)> {
        (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::Frame { rect, mode, time } => Some((rect.x, rect.y, mode, time != 0)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_first_list_has_its_heading_its_lines_and_the_help_of_the_cursor() {
        let mut settings = Settings::at_boot();
        let mut screen = OptionsScreen::open(&settings);
        screen.pass(&mut settings, &Pad::default());
        screen.pass(&mut settings, &press(pad::DOWN));
        let drawn = drawn(&screen, &settings, &lines(), 7, |_| 40);
        // The lines of a list whose cursor pulses are all of the kind that can.
        assert_eq!(
            frames(&drawn),
            [
                (125, 18, 0, false),
                (114, 81, 1, false),
                (114, 101, 1, true),
                (114, 121, 1, false),
                (114, 141, 1, false),
                (10, 194, 0, false),
            ]
        );
        // The heading's word, and a line's picture in the middle of its room.
        assert_eq!(
            drawn[1],
            Drawn::Sprite {
                number: 79,
                at: (132, 21),
                clip: Rect {
                    x: 0,
                    y: 0,
                    w: 320,
                    h: 240
                }
            }
        );
        assert!(matches!(
            drawn[3],
            Drawn::Sprite {
                number: 221,
                at: (140, 84),
                ..
            }
        ));
        assert_eq!(
            drawn.last(),
            Some(&Drawn::OptionsHelp {
                page: 0,
                line: 1,
                at: HELP_TEXT
            })
        );
    }

    #[test]
    fn a_page_marks_the_values_and_turns_the_styles_round_while_one_is_picked() {
        let mut settings = Settings::at_boot();
        let mut screen = OptionsScreen::open(&settings);
        screen.pass(&mut settings, &Pad::default());
        screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!(screen.page, AUDIO);
        let drawn_page = drawn(&screen, &settings, &lines(), 7, |_| 10);
        let frames_of = frames(&drawn_page);
        // The heading of a page, the label under the cursor, and the values of a new
        // game in the marked colour: the music at 5, the effects at 4, Stereo.
        assert_eq!(frames_of[0], (114, 18, 0, false));
        assert_eq!(frames_of[1], (36, 83, 1, true));
        let marked: Vec<(i16, i16)> = (frames_of.iter())
            .filter(|frame| frame.2 == 2)
            .map(|frame| (frame.0, frame.1))
            .collect();
        assert_eq!(
            marked,
            [(118 + 19 * 5, 83), (118 + 19 * 4, 103), (125, 123)]
        );
        // While the music is picked its value pulses and the label is marked.
        screen.pass(&mut settings, &press(pad::CROSS));
        assert!(screen.picking);
        let picked = frames(&drawn(&screen, &settings, &lines(), 7, |_| 10));
        assert_eq!(picked[1], (36, 83, 2, false));
        assert!(picked.contains(&(118 + 19 * 5, 83, 1, true)));
        assert!(picked.contains(&(118 + 19 * 4, 103, 2, false)));
    }

    #[test]
    fn the_rows_that_need_another_are_dim_and_a_refusal_takes_the_place_of_the_help() {
        let mut settings = Settings::at_boot();
        settings.word &= !(1 << 22);
        let mut screen = OptionsScreen::open(&settings);
        screen.pass(&mut settings, &Pad::default());
        screen.pass(&mut settings, &press(pad::DOWN));
        screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!(screen.page, SUBTITLES);
        let dim = |drawn: &[Drawn]| -> Vec<i16> {
            (drawn.iter())
                .filter_map(|drawn| match *drawn {
                    Drawn::DimSprite { clip, .. } => Some(clip.y),
                    _ => None,
                })
                .collect()
        };
        // With Speech OFF the label of Auto Play alone is dim.
        assert_eq!(dim(&drawn(&screen, &settings, &lines(), 0, |_| 10)), [101]);
        for _ in 0..3 {
            screen.pass(&mut settings, &press(pad::DOWN));
        }
        screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!(screen.refusal, options_screen::NEEDS_SPEECH);
        let refused = drawn(&screen, &settings, &lines(), 0, |_| 10);
        assert!(refused.contains(&Drawn::Frame {
            rect: REFUSAL_BOX,
            mode: 0,
            time: 0
        }));
        assert_eq!(
            refused[refused.len() - 2..],
            [
                Drawn::OptionsRefused {
                    which: 1,
                    line: 0,
                    at: (13, 196)
                },
                Drawn::OptionsRefused {
                    which: 1,
                    line: 1,
                    at: (13, 208)
                }
            ]
        );
    }

    #[test]
    fn the_page_of_the_colours_shows_all_four_and_pulses_under_the_cursor() {
        let mut settings = Settings::at_boot();
        settings.colour = 2;
        let mut screen = OptionsScreen::open(&settings);
        screen.pass(&mut settings, &Pad::default());
        screen.pass(&mut settings, &press(pad::DOWN));
        screen.pass(&mut settings, &press(pad::DOWN));
        screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!((screen.page, screen.cursor.at), (COLOUR, 2));
        let drawn = drawn(&screen, &settings, &lines(), 9, |_| 96);
        let of: Vec<(i16, i16, i16, u8, u32)> = (drawn.iter())
            .filter_map(|drawn| match *drawn {
                Drawn::FrameOf {
                    rect,
                    colour,
                    mode,
                    time,
                } => Some((colour, rect.x, rect.y, mode, time)),
                _ => None,
            })
            .collect();
        assert_eq!(of.len(), 8);
        assert_eq!(of[4..6], [(2, 102, 118, 0, 0), (2, 82, 118, 1, 9)]);
        assert_eq!(of[6..], [(3, 102, 156, 0, 0), (3, 82, 156, 1, 0)]);
        assert!(drawn.contains(&Drawn::Sprite {
            number: LOGO,
            at: (116, 44 + 76),
            clip: Rect {
                x: 0,
                y: 0,
                w: 320,
                h: 240
            }
        }));
    }
}
