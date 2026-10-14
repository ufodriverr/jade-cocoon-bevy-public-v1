
use crate::menu::{self, Cursor};
use crate::name::Cells;
use crate::pad::Pad;
use crate::player_menu::{
    MESSAGE_PASSES, SOUND_CANCEL, SOUND_CONFIRM, SOUND_CURSOR, SOUND_REFUSED,
};

/// The archive file of the screen and the RAM address of its first byte.
pub const OPTIONS_FILE: usize = 47;
pub const OPTIONS_FILE_BASE: u32 = 0x800C_2BB8;

/// The pages: the first list and the page behind each of its four lines.
pub const FIRST: i16 = 0;
pub const AUDIO: i16 = 1;
pub const SUBTITLES: i16 = 2;
pub const COLOUR: i16 = 3;
pub const VIBRATION: i16 = 4;
/// What is asked for in place of a page to end the screen, and for nothing.
const END: i16 = 5;
const NONE: i16 = -1;

/// The cursor record of each page: columns, rows, lines, flag word (`0x800C4EDC`).
pub const CURSORS: [[i16; 4]; 5] = [
    [1, 4, 4, 0],
    [1, 3, 3, 0],
    [1, 7, 7, 0],
    [1, 4, 4, 0],
    [1, 1, 1, 0],
];
/// The eight bytes behind those records, which the start copies as the cursor record of
/// the first pass: the head of the first line's piece.
pub const CURSOR_OF_THE_START: [i16; 4] = [8, 1, 221, 0];

/// The rows of the page of the sound.
pub const MUSIC: i16 = 0;
pub const EFFECTS: i16 = 1;
pub const OUTPUT: i16 = 2;
/// The rows of the page of the texts and effects.
pub const DISPLAY: i16 = 0;
pub const ANIM_CUT: i16 = 1;
pub const SPEECH: i16 = 2;
pub const AUTO_PLAY: i16 = 3;
pub const SUBTITLE: i16 = 4;
pub const MAGIC_FX: i16 = 5;
pub const CAPTURE_FX: i16 = 6;

/// The refusals of a confirm: the row needs Subtitles, the row needs Speech.
pub const NEEDS_SUBTITLES: u8 = 1;
pub const NEEDS_SPEECH: u8 = 2;

/// The steps of a volume: Mute and 1 to 7, each sixteen of the 127.
pub const VOLUME_STEPS: i16 = 8;

/// What the screen reads and writes of the game's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// The setting word `0x8008B51C`.
    pub word: u32,
    /// The window colour, the half word `0x8008B520`.
    pub colour: u16,
    /// Bit `0x40` of the word `0x8008B500`: the pad is not shaken.
    pub vibration_off: bool,
}

impl Settings {
    pub fn at_boot() -> Self {
        Self {
            word: crate::window::SETTING_AT_START,
            colour: 0,
            vibration_off: false,
        }
    }

    /// The step of the music's volume, bits 7 to 13 of the word over 16.
    pub fn music(&self) -> u8 {
        (((self.word >> 7) & 0x7F) >> 4) as u8
    }

    /// The step of the effects' volume, bits 14 to 20 over 16.
    pub fn effects(&self) -> u8 {
        (((self.word >> 14) & 0x7F) >> 4) as u8
    }

    fn set_music(&mut self, step: u8) {
        self.word = (self.word & !0x3F80) | ((u32::from(step) << 11) & 0x3800);
    }

    fn set_effects(&mut self, step: u8) {
        self.word = (self.word & 0xFFE0_3FFF) | (((u32::from(step) << 4) & 0x7F) << 14);
    }

    fn set_bit(&mut self, bit: u32, value: u8) {
        self.word = (self.word & !(1 << bit)) | (u32::from(value & 1) << bit);
    }
}

/// The settings as the bytes of the task's record (+8 to +22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bytes {
    pub display: u8,
    /// The two bytes of "Anim. Cut", +9 and +10.
    pub anim_cut: [u8; 2],
    pub magic_fx: u8,
    pub output: u8,
    pub music: u8,
    pub effects: u8,
    pub colour: u8,
    pub capture_fx: u8,
    pub speech: u8,
    pub vibration: u8,
    pub auto_play: u8,
    pub subtitles: u8,
    /// The two volumes as they were when the bytes were read (+21, +22).
    pub music_before: u8,
    pub effects_before: u8,
}

impl Bytes {
    /// The reader `0x800C3248`. Speech and Auto Play read as ON while Subtitles is OFF,
    /// and Auto Play as OFF while Speech is.
    pub fn read(settings: &Settings) -> Self {
        let bit = |n: u32| ((settings.word >> n) & 1) as u8;
        let subtitles = bit(24);
        let speech = if subtitles == 0 { 1 } else { bit(22) };
        let auto_play = match (subtitles, speech) {
            (_, 0) => 0,
            (0, _) => 1,
            _ => bit(23),
        };
        Self {
            display: (settings.word & 7) as u8,
            anim_cut: [bit(3); 2],
            magic_fx: bit(5),
            output: bit(6),
            music: settings.music(),
            effects: settings.effects(),
            colour: settings.colour as u8,
            capture_fx: bit(21),
            speech,
            vibration: u8::from(!settings.vibration_off),
            auto_play,
            subtitles,
            music_before: settings.music(),
            effects_before: settings.effects(),
        }
    }
}

/// What a pass of the screen did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Passed {
    /// The sounds asked for, in their order.
    pub sounds: Vec<u8>,
    /// The sound library was told one or two loudspeakers anew (`0x800309E4`).
    pub output_told: bool,
    /// The task ended in this pass: whoever waits for the screen goes on.
    pub ended: bool,
}

/// The record of the screen's task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsScreen {
    /// The page (+2) and the page asked for (+4).
    pub page: i16,
    asked: i16,
    /// The kept cursor of the first list (+6).
    pub kept: i16,
    pub bytes: Bytes,
    /// A refusal is up (+48): 0, [`NEEDS_SUBTITLES`] or [`NEEDS_SPEECH`].
    pub refusal: u8,
    /// The passes the refusal has been up (+52).
    pub refusal_passes: i16,
    /// An answer of the cursor's row is being picked (+56).
    pub picking: bool,
    pub cursor: Cursor,
    /// The word `0x800C5B4C`: the screen is up.
    pub up: bool,
}

impl OptionsScreen {
    /// The screen as its start leaves it (`0x800C309C`): the first list asked for, the
    /// cursor record not yet a page's.
    pub fn open(settings: &Settings) -> Self {
        let [columns, rows, lines, flags] = CURSOR_OF_THE_START;
        Self {
            page: FIRST,
            asked: FIRST,
            kept: 0,
            bytes: Bytes::read(settings),
            refusal: 0,
            refusal_passes: 0,
            picking: false,
            cursor: Cursor {
                columns,
                rows,
                lines: lines as u16,
                flags: flags as u16,
                at: 0,
                first: 0,
            },
            up: true,
        }
    }

    /// One pass of the task with the pad's words of the pass: `0x800C3158`, or
    /// `0x800C47F0` while an answer is picked and `0x800C4D3C` while a refusal is up.
    pub fn pass(&mut self, settings: &mut Settings, pad: &Pad) -> Passed {
        let mut passed = Passed::default();
        if !self.up {
            return passed;
        }
        if self.refusal != 0 {
            self.pass_refusal(pad, &mut passed);
            return passed;
        }
        if self.picking {
            self.pick(settings, pad, &mut passed);
            return passed;
        }
        let before = self.cursor.at;
        if self.cursor.step(pad.repeated).sounds {
            passed.sounds.push(SOUND_CURSOR);
        }
        if self.cursor.at != before {
            self.bytes = Bytes::read(settings);
        }
        self.input(settings, pad, &mut passed);
        match self.asked {
            NONE => {}
            END => {
                self.up = false;
                passed.ended = true;
            }
            page => self.enter(page, settings),
        }
        passed
    }

    /// A page asked for is entered (`0x800C3994`): its cursor record, and coming from
    /// the first list the cursor on the first row, or on the colour in use.
    fn enter(&mut self, page: i16, settings: &Settings) {
        let Some(&[columns, rows, lines, flags]) =
            (usize::try_from(page).ok()).and_then(|page| CURSORS.get(page))
        else {
            return;
        };
        self.cursor.columns = columns;
        self.cursor.rows = rows;
        self.cursor.lines = lines as u16;
        self.cursor.flags = flags as u16;
        if page == FIRST {
            self.cursor.first = 0;
        } else if self.page == FIRST {
            self.cursor.at = match page {
                COLOUR => settings.colour as i16,
                _ => 0,
            };
            self.cursor.first = 0;
        }
        self.page = page;
        self.asked = NONE;
    }

    /// The pad is read (`0x800C3748`, a jump by the page).
    fn input(&mut self, settings: &mut Settings, pad: &Pad, passed: &mut Passed) {
        match self.page {
            FIRST => {
                if menu::confirmed(pad) {
                    passed.sounds.push(SOUND_CONFIRM);
                    self.asked = self.cursor.at + 1;
                    self.kept = self.cursor.at;
                } else if menu::cancelled(pad) {
                    passed.sounds.push(SOUND_CANCEL);
                    self.asked = END;
                }
            }
            AUDIO | VIBRATION => {
                if menu::confirmed(pad) {
                    passed.sounds.push(SOUND_CONFIRM);
                    self.picking = true;
                } else {
                    self.back(settings, pad, passed);
                }
            }
            SUBTITLES => {
                if !menu::confirmed(pad) {
                    self.back(settings, pad, passed);
                    return;
                }
                let row = self.cursor.at;
                let refusal = if self.bytes.subtitles == 0 && (row == SPEECH || row == AUTO_PLAY) {
                    NEEDS_SUBTITLES
                } else if self.bytes.speech == 0 && row == AUTO_PLAY {
                    NEEDS_SPEECH
                } else {
                    0
                };
                if refusal != 0 {
                    passed.sounds.push(SOUND_REFUSED);
                    self.refusal = refusal;
                } else {
                    passed.sounds.push(SOUND_CONFIRM);
                    self.picking = true;
                }
            }
            COLOUR => {
                self.bytes.colour = self.cursor.at as u8;
                if menu::confirmed(pad) {
                    passed.sounds.push(SOUND_CONFIRM);
                    self.write(settings, passed);
                } else {
                    self.back(settings, pad, passed);
                }
            }
            _ => {}
        }
    }

    /// A cancel on a page: the settings are read again and the first list is asked for
    /// with its kept cursor.
    fn back(&mut self, settings: &Settings, pad: &Pad, passed: &mut Passed) {
        if !menu::cancelled(pad) {
            return;
        }
        passed.sounds.push(SOUND_CANCEL);
        self.bytes = Bytes::read(settings);
        self.asked = FIRST;
        self.cursor.at = self.kept;
    }

    /// The byte the cursor's row picks an answer for, and how many answers it has. The
    /// two volumes have none: they are picked on the setting word itself.
    fn answer(&mut self) -> Option<(&mut u8, i16)> {
        let bytes = &mut self.bytes;
        Some(match (self.page, self.cursor.at) {
            (AUDIO, OUTPUT) => (&mut bytes.output, 2),
            (SUBTITLES, DISPLAY) => (&mut bytes.display, 4),
            (SUBTITLES, ANIM_CUT) => (&mut bytes.anim_cut[0], 2),
            (SUBTITLES, SPEECH) => (&mut bytes.speech, 2),
            (SUBTITLES, AUTO_PLAY) => (&mut bytes.auto_play, 2),
            (SUBTITLES, SUBTITLE) => (&mut bytes.subtitles, 2),
            (SUBTITLES, MAGIC_FX) => (&mut bytes.magic_fx, 2),
            (SUBTITLES, CAPTURE_FX) => (&mut bytes.capture_fx, 2),
            (VIBRATION, _) => (&mut bytes.vibration, 2),
            _ => return None,
        })
    }

    /// One step of the executable's second cursor mover over a row of answers that does
    /// not wrap: the new value and whether it moved.
    fn moved(value: u8, answers: i16, pad: &Pad) -> (u8, bool) {
        let cells = Cells {
            flags: 0,
            columns: answers,
            rows: 1,
        };
        let mut at = i16::from(value);
        cells.step(&mut at, pad.repeated);
        (at as u8, at != i16::from(value))
    }

    /// A pass while an answer is picked (`0x800C47F0`). A volume is written to the
    /// setting word with every pass; a confirm writes the row, a cancel puts the
    /// volumes back and reads the settings again.
    fn pick(&mut self, settings: &mut Settings, pad: &Pad, passed: &mut Passed) {
        let mut sounds = false;
        match (self.page, self.cursor.at) {
            (AUDIO, MUSIC) => {
                let (step, moved) = Self::moved(settings.music(), VOLUME_STEPS, pad);
                settings.set_music(step);
                sounds = moved;
            }
            (AUDIO, EFFECTS) => {
                let (step, moved) = Self::moved(settings.effects(), VOLUME_STEPS, pad);
                settings.set_effects(step);
                sounds = moved;
            }
            _ => {
                if let Some((byte, answers)) = self.answer() {
                    let (value, moved) = Self::moved(*byte, answers, pad);
                    *byte = value;
                    sounds = moved;
                }
                if (self.page, self.cursor.at) == (SUBTITLES, ANIM_CUT) {
                    self.bytes.anim_cut[1] = self.bytes.anim_cut[0];
                }
            }
        }
        if sounds {
            passed.sounds.push(SOUND_CURSOR);
        }
        if menu::confirmed(pad) {
            passed.sounds.push(SOUND_CONFIRM);
            self.write(settings, passed);
            self.picking = false;
        } else if menu::cancelled(pad) {
            passed.sounds.push(SOUND_CANCEL);
            settings.set_music(self.bytes.music_before);
            settings.set_effects(self.bytes.effects_before);
            self.bytes = Bytes::read(settings);
            self.picking = false;
        }
    }

    /// The cursor's row is written to the game's settings (`0x800C33C4`, a jump by the
    /// page and the row). Speech put to OFF puts Auto Play to OFF; Subtitles put to OFF
    /// puts Speech and Auto Play to ON.
    fn write(&mut self, settings: &mut Settings, passed: &mut Passed) {
        let bytes = &mut self.bytes;
        match (self.page, self.cursor.at) {
            (AUDIO, MUSIC) => bytes.music_before = settings.music(),
            (AUDIO, EFFECTS) => bytes.effects_before = settings.effects(),
            (AUDIO, OUTPUT) => {
                settings.set_bit(6, bytes.output);
                passed.output_told = true;
            }
            (SUBTITLES, DISPLAY) => {
                settings.word = (settings.word & !7) | u32::from(bytes.display & 7);
            }
            (SUBTITLES, ANIM_CUT) => {
                settings.set_bit(3, bytes.anim_cut[0]);
                settings.set_bit(4, bytes.anim_cut[1]);
            }
            (SUBTITLES, SPEECH) => {
                settings.set_bit(22, bytes.speech);
                if bytes.speech & 1 == 0 {
                    bytes.auto_play = 0;
                    settings.set_bit(23, 0);
                }
            }
            (SUBTITLES, AUTO_PLAY) => settings.set_bit(23, bytes.auto_play),
            (SUBTITLES, SUBTITLE) => {
                settings.set_bit(24, bytes.subtitles);
                if bytes.subtitles & 1 == 0 {
                    bytes.speech = 1;
                    bytes.auto_play = 1;
                    settings.set_bit(22, 1);
                    settings.set_bit(23, 1);
                }
            }
            (SUBTITLES, MAGIC_FX) => settings.set_bit(5, bytes.magic_fx),
            (SUBTITLES, CAPTURE_FX) => settings.set_bit(21, bytes.capture_fx),
            (COLOUR, _) => settings.colour = u16::from(bytes.colour),
            (VIBRATION, _) => settings.vibration_off = bytes.vibration == 0,
            _ => {}
        }
    }

    /// A pass while a refusal is up (`0x800C4D3C`): it ends on a confirm or a cancel, or
    /// in the pass that finds its passes counted.
    fn pass_refusal(&mut self, pad: &Pad, passed: &mut Passed) {
        if self.refusal_passes >= MESSAGE_PASSES || menu::confirmed(pad) || menu::cancelled(pad) {
            passed.sounds.push(SOUND_CANCEL);
            self.refusal = 0;
            self.refusal_passes = 0;
        } else {
            self.refusal_passes += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::{self, Repeat};
    use crate::window::SETTING_AT_START;

    /// The pad's words of a pass in which the buttons went down.
    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(buttons, Repeat::of(1), 1);
        pad
    }

    fn new_game() -> Settings {
        Settings {
            word: SETTING_AT_START,
            colour: 0,
            vibration_off: false,
        }
    }

    /// A screen on a page with the cursor on a row, as the first list leads there.
    fn on(page: i16, row: i16, settings: &mut Settings) -> OptionsScreen {
        let mut screen = OptionsScreen::open(settings);
        screen.pass(settings, &press(0));
        for _ in 0..page - 1 {
            screen.pass(settings, &press(pad::DOWN));
        }
        screen.pass(settings, &press(pad::CROSS));
        for _ in 0..row {
            screen.pass(settings, &press(pad::DOWN));
        }
        assert_eq!((screen.page, screen.cursor.at), (page, row));
        screen
    }

    #[test]
    fn the_bytes_of_a_new_game_and_what_they_hide() {
        let mut settings = new_game();
        let bytes = Bytes::read(&settings);
        assert_eq!((bytes.music, bytes.effects, bytes.output), (5, 4, 0));
        assert_eq!((bytes.display, bytes.anim_cut), (2, [1, 1]));
        assert_eq!((bytes.speech, bytes.auto_play, bytes.subtitles), (1, 1, 1));
        assert_eq!(
            (bytes.magic_fx, bytes.capture_fx, bytes.vibration),
            (1, 1, 1)
        );
        // Without Subtitles both read as ON, whatever the word says.
        settings.word &= !(0b111 << 22);
        let bytes = Bytes::read(&settings);
        assert_eq!((bytes.speech, bytes.auto_play, bytes.subtitles), (1, 1, 0));
        // Without Speech Auto Play reads as OFF.
        settings.word = SETTING_AT_START & !(1 << 22);
        let bytes = Bytes::read(&settings);
        assert_eq!((bytes.speech, bytes.auto_play, bytes.subtitles), (0, 0, 1));
        settings.vibration_off = true;
        assert_eq!(Bytes::read(&settings).vibration, 0);
    }

    #[test]
    fn the_first_list_leads_to_its_pages_and_back_and_out() {
        let mut settings = new_game();
        settings.colour = 2;
        let mut screen = OptionsScreen::open(&settings);
        // The first pass moves the cursor by the record of the start, then enters.
        assert_eq!(screen.cursor.columns, 8);
        screen.pass(&mut settings, &press(0));
        assert_eq!((screen.cursor.rows, screen.cursor.lines), (4, 4));
        // No cursor wraps: up from the first line stays there, without a sound.
        assert!(
            screen
                .pass(&mut settings, &press(pad::UP))
                .sounds
                .is_empty()
        );
        for _ in 0..2 {
            let passed = screen.pass(&mut settings, &press(pad::DOWN));
            assert_eq!(passed.sounds, [SOUND_CURSOR]);
        }
        let passed = screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        // The page of the colour starts on the colour in use.
        assert_eq!((screen.page, screen.cursor.at, screen.kept), (COLOUR, 2, 2));
        screen.pass(&mut settings, &press(pad::UP));
        assert_eq!(screen.bytes.colour, 1);
        assert_eq!(settings.colour, 2);
        // A confirm stores the colour and the page stays.
        let passed = screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!((passed.sounds, settings.colour), (vec![SOUND_CONFIRM], 1));
        assert_eq!(screen.page, COLOUR);
        // A cancel goes back to the kept line, a second one ends the screen.
        let passed = screen.pass(&mut settings, &press(pad::TRIANGLE));
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
        assert_eq!((screen.page, screen.cursor.at), (FIRST, 2));
        let passed = screen.pass(&mut settings, &press(pad::TRIANGLE));
        assert!(passed.ended && !screen.up);
        assert_eq!(
            screen.pass(&mut settings, &press(pad::CROSS)),
            Passed::default()
        );
    }

    #[test]
    fn a_volume_is_written_while_it_is_picked_and_put_back_by_a_cancel() {
        let mut settings = new_game();
        let mut screen = on(AUDIO, MUSIC, &mut settings);
        screen.pass(&mut settings, &press(pad::CROSS));
        assert!(screen.picking);
        let passed = screen.pass(&mut settings, &press(pad::RIGHT));
        assert_eq!((passed.sounds, settings.music()), (vec![SOUND_CURSOR], 6));
        // The row's cursor stands still, and the steps end at 7 without a sound.
        screen.pass(&mut settings, &press(pad::DOWN));
        assert_eq!(screen.cursor.at, MUSIC);
        screen.pass(&mut settings, &press(pad::RIGHT));
        assert!(
            screen
                .pass(&mut settings, &press(pad::RIGHT))
                .sounds
                .is_empty()
        );
        assert_eq!(settings.music(), 7);
        let passed = screen.pass(&mut settings, &press(pad::TRIANGLE));
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
        assert_eq!((settings.word, screen.picking), (SETTING_AT_START, false));
        // A confirm keeps the new volume as the one to go back to.
        screen.pass(&mut settings, &press(pad::DOWN));
        screen.pass(&mut settings, &press(pad::CROSS));
        screen.pass(&mut settings, &press(pad::LEFT));
        screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!((settings.effects(), screen.bytes.effects_before), (3, 3));
        assert_eq!(settings.music(), 5);
        // The output format tells the sound library.
        screen.pass(&mut settings, &press(pad::DOWN));
        screen.pass(&mut settings, &press(pad::CROSS));
        screen.pass(&mut settings, &press(pad::RIGHT));
        assert_eq!(settings.word & 0x40, 0);
        let passed = screen.pass(&mut settings, &press(pad::CROSS));
        assert!(passed.output_told);
        assert_eq!(settings.word & 0x40, 0x40);
    }

    #[test]
    fn rows_that_change_other_rows_and_the_two_refusals() {
        let mut settings = new_game();
        let mut screen = on(SUBTITLES, SPEECH, &mut settings);
        // Speech put to OFF puts Auto Play to OFF.
        screen.pass(&mut settings, &press(pad::CROSS));
        screen.pass(&mut settings, &press(pad::LEFT));
        assert_ne!(settings.word & (1 << 22), 0);
        screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!(settings.word & (0b11 << 22), 0);
        assert_eq!(screen.bytes.auto_play, 0);
        // Auto Play is refused while Speech is OFF.
        screen.pass(&mut settings, &press(pad::DOWN));
        let passed = screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!(passed.sounds, [SOUND_REFUSED]);
        assert_eq!((screen.refusal, screen.picking), (NEEDS_SPEECH, false));
        // The refusal is deaf to the cursor and ends on a button, with the cancel's sound.
        screen.pass(&mut settings, &press(pad::DOWN));
        assert_eq!((screen.cursor.at, screen.refusal_passes), (AUTO_PLAY, 1));
        let passed = screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!((passed.sounds, screen.refusal), (vec![SOUND_CANCEL], 0));
        // Subtitles put to OFF puts Speech and Auto Play to ON, and both are refused.
        screen.pass(&mut settings, &press(pad::DOWN));
        screen.pass(&mut settings, &press(pad::CROSS));
        screen.pass(&mut settings, &press(pad::LEFT));
        screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!((settings.word >> 22) & 0b111, 0b011);
        for _ in 0..2 {
            screen.pass(&mut settings, &press(pad::UP));
            screen.pass(&mut settings, &press(pad::CROSS));
            assert_eq!(screen.refusal, NEEDS_SUBTITLES);
            screen.pass(&mut settings, &press(pad::TRIANGLE));
        }
        assert_eq!(screen.cursor.at, SPEECH);
        // Left alone a refusal ends in the pass that finds its passes counted.
        screen.pass(&mut settings, &press(pad::CROSS));
        for _ in 0..MESSAGE_PASSES {
            assert!(screen.pass(&mut settings, &press(0)).sounds.is_empty());
        }
        assert_eq!(screen.pass(&mut settings, &press(0)).sounds, [SOUND_CANCEL]);
        assert_eq!(screen.refusal, 0);
    }

    #[test]
    fn the_other_rows_write_their_bits_only_on_a_confirm() {
        let mut settings = new_game();
        let mut screen = on(SUBTITLES, DISPLAY, &mut settings);
        screen.pass(&mut settings, &press(pad::CROSS));
        screen.pass(&mut settings, &press(pad::RIGHT));
        assert_eq!((screen.bytes.display, settings.word & 7), (3, 2));
        // A cancel reads the byte again from the word.
        screen.pass(&mut settings, &press(pad::TRIANGLE));
        assert_eq!(screen.bytes.display, 2);
        screen.pass(&mut settings, &press(pad::CROSS));
        screen.pass(&mut settings, &press(pad::LEFT));
        screen.pass(&mut settings, &press(pad::LEFT));
        screen.pass(&mut settings, &press(pad::CROSS));
        assert_eq!(settings.word & 7, 0);
        for (row, bits) in [(ANIM_CUT, 0x18), (MAGIC_FX, 1 << 5), (CAPTURE_FX, 1 << 21)] {
            let mut settings = new_game();
            let mut screen = on(SUBTITLES, row, &mut settings);
            screen.pass(&mut settings, &press(pad::CROSS));
            screen.pass(&mut settings, &press(pad::LEFT));
            screen.pass(&mut settings, &press(pad::CROSS));
            assert_eq!(settings.word, SETTING_AT_START & !bits, "row {row}");
        }
        let mut settings = new_game();
        let mut screen = on(VIBRATION, 0, &mut settings);
        screen.pass(&mut settings, &press(pad::CROSS));
        screen.pass(&mut settings, &press(pad::LEFT));
        assert!(!settings.vibration_off);
        screen.pass(&mut settings, &press(pad::CROSS));
        assert!(settings.vibration_off);
    }
}
