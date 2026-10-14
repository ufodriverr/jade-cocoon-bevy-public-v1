
use crate::carried::Carried;
use crate::items::{FREE, Keeping, Kind, MOST_OF_A_THING};
use crate::menu::{self, Cursor, cursor};
use crate::pad::{self, Pad};
use crate::player_menu::{
    MESSAGE_PASSES, OPEN, SOUND_CANCEL, SOUND_CONFIRM, SOUND_CURSOR, SOUND_REFUSED,
};
use crate::rooms::Image;

/// The archive file of the screen and the RAM address of its first byte.
pub const SHOP_FILE: usize = 46;
pub const SHOP_FILE_BASE: u32 = 0x800B_6890;

/// The two ways of the entry `0x800BAFFC`: the kinds 0 and 1 of script command `0x59`.
pub const BLACKSMITH: i16 = 0;
pub const SHOP: i16 = 1;

/// The states.
pub const STATES: usize = 19;
/// The first menu of the shop and of the blacksmith: buy, sell, leave.
pub const FIRST_OF_SHOP: i16 = 0;
pub const FIRST_OF_BLACKSMITH: i16 = 1;
/// The shop's offer and the player's items.
pub const OFFER: i16 = 2;
pub const ITEMS: i16 = 3;
/// The blacksmith's offer of a kind and the player's list of it.
pub const OFFER_OF_KIND: i16 = 4;
pub const LIST_OF_KIND: i16 = 5;
/// The blacksmith's three kinds, to buy and to sell.
pub const KINDS_TO_BUY: i16 = 6;
pub const KINDS_TO_SELL: i16 = 7;
/// The amount: the four states from here, the blacksmith's to buy and to sell, then
/// the shop's.
pub const AMOUNT: i16 = 8;
/// The question: the four states from here, in the same order.
pub const QUESTION: i16 = 12;
/// Whether to put the bought thing on, and the keeper's line behind a yes and a no.
pub const PUT_ON: i16 = 16;
pub const PUT_ON_DONE: i16 = 17;
pub const PUT_ON_LATER: i16 = 18;

/// The panel that has the cursor in each state (the half words at `0x800BE9E8`).
pub const PANEL_OF: [i16; STATES] = [
    0, 6, 4, 5, 10, 11, 9, 9, 13, 14, 17, 18, 15, 12, 19, 16, 26, 27, 28,
];
/// The keeper's line of each state (the half words at `0x800BEA10`).
pub const LINE_OF: [i16; STATES] = [
    9, 0, 10, 11, 3, 4, 1, 2, 5, 6, 12, 13, 7, 8, 14, 15, 20, 21, 22,
];
/// The panels of each state, a bit a panel (the words at `0x800BE99C`).
pub const SHOWN: [u32; STATES] = [
    0x0080_0005,
    0x0080_0044,
    0x00A0_001A,
    0x00A0_002A,
    0x00D0_0488,
    0x0090_0888,
    0x0080_0204,
    0x0080_0204,
    0x02D0_2088,
    0x0290_4088,
    0x02A2_000A,
    0x02A4_000A,
    0x02D0_2488,
    0x0290_4888,
    0x02A2_001A,
    0x02A4_002A,
    0x00D0_0400,
    0x00D0_0400,
    0x00D0_0400,
];
/// The cursor record of each state: columns, rows, lines, flag word (through the panel
/// table `0x800BE750`). The lines of a list are counted; the states of the amount have
/// none.
pub const CURSORS: [Option<[i16; 4]>; STATES] = [
    Some(MENU),
    Some(MENU),
    Some([1, 4, 4, cursor::SCROLLS as i16]),
    Some([1, 4, 64, cursor::SCROLLS as i16]),
    Some([1, 4, 4, cursor::SCROLLS as i16]),
    Some([1, 4, 64, cursor::SCROLLS as i16]),
    Some(MENU),
    Some(MENU),
    None,
    None,
    None,
    None,
    Some(ANSWER),
    Some(ANSWER),
    Some(ANSWER),
    Some(ANSWER),
    Some(ANSWER),
    Some([1, 1, 1, 0]),
    Some([1, 1, 1, 0]),
];
const MENU: [i16; 4] = [1, 3, 3, 0];
const ANSWER: [i16; 4] = [1, 2, 2, 0];

/// The states behind the first menu of the blacksmith and of the shop, and the lists of
/// the blacksmith, each to buy and to sell (the half words at `0x800BEA38`).
pub const BEHIND_BLACKSMITH: [i16; 2] = [KINDS_TO_BUY, KINDS_TO_SELL];
pub const BEHIND_SHOP: [i16; 2] = [OFFER, ITEMS];
pub const LISTS_OF_KIND: [i16; 2] = [OFFER_OF_KIND, LIST_OF_KIND];

/// The row of the first menu that leaves, and the rows of an answer.
pub const LEAVE: i16 = 2;
pub const YES: i16 = 0;
pub const NO: i16 = 1;
/// The rows of a list.
pub const LIST_ROWS: i16 = 4;
/// The refusals: the money is short, nothing more can be carried.
pub const NO_MONEY: i16 = 1;
pub const NO_ROOM: i16 = 2;
/// The keeper's line of a refusal at the shop's offer and at the blacksmith's, each for
/// the two refusals (`0x800BC024`).
pub const REFUSAL_OF_SHOP: [i16; 2] = [17, 18];
pub const REFUSAL_OF_BLACKSMITH: [i16; 2] = [16, 19];
/// The step of R2 and R1 on the amount.
pub const BIG_STEP: i32 = 10;

/// The tables of the file's stocks and of the executable's prices, each indexed by the
/// list of [`Kind`]: items, weapons, armours, ornaments.
const STOCKS: [(u32, usize); 4] = [
    (0x800B_F3BC, 43),
    (0x800B_F414, 23),
    (0x800B_F444, 15),
    (0x800B_F464, 17),
];
const ITEM_PRICES: u32 = 0x8007_CDFC;
const WEAPON_PRICES: u32 = 0x8007_D2AC;
const ARMOUR_PRICES: u32 = 0x8007_D570;
const ORNAMENT_PRICES: u32 = 0x8007_D838;

/// What is for sale and what it costs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wares {
    /// A word a thing of each list: the bits of the stocks that offer it.
    pub offered: [Vec<u16>; 4],
    /// The price of each thing of each list.
    pub prices: [Vec<i32>; 4],
}

impl Wares {
    /// Reads the stocks from archive file 46 and the prices from the loaded executable.
    pub fn read(file: &[u8], executable: &[u8], base: u32) -> Option<Self> {
        let file = Image {
            data: file,
            base: SHOP_FILE_BASE,
        };
        let exe = Image {
            data: executable,
            base,
        };
        let mut wares = Self::default();
        for (list, (table, count)) in STOCKS.into_iter().enumerate() {
            for thing in 0..count as u32 {
                wares.offered[list].push(file.u16(table + 2 * thing).ok()?);
                let price = match list {
                    0 => i32::from(exe.u16(ITEM_PRICES + 2 * thing).ok()?),
                    // A weapon's word is a tenth of its price.
                    1 => exe.i32(WEAPON_PRICES + 4 * thing).ok()?.wrapping_mul(10),
                    2 => i32::from(exe.u16(ARMOUR_PRICES + 2 * thing).ok()?),
                    _ => i32::from(exe.u16(ORNAMENT_PRICES + 2 * thing).ok()?),
                };
                wares.prices[list].push(price);
            }
        }
        Some(wares)
    }

    /// The price of a thing (`0x8001CEBC`, `0x8001D260`); 0 for a number past its list.
    pub fn price(&self, thing: u8, list: Kind) -> i32 {
        let prices = self.prices.get(list as usize);
        prices
            .and_then(|prices| prices.get(usize::from(thing)))
            .copied()
            .unwrap_or(0)
    }

    /// What a thing brings when it is sold: three quarters of its price.
    pub fn worth(&self, thing: u8, list: Kind) -> i32 {
        self.price(thing, list).wrapping_mul(3) / 4
    }

    /// How many things of a list a stock offers (`0x800BDFC8`).
    pub fn offers(&self, list: Kind, stock: u16) -> i16 {
        self.on_offer(list, stock).count() as i16
    }

    /// The n-th thing of a list that a stock offers (`0x800BDF80`, `0x800BDEFC`), and
    /// [`FREE`] past the offer, where the game reads on behind its table.
    pub fn nth(&self, n: i16, list: Kind, stock: u16) -> u8 {
        let n = usize::try_from(n).ok();
        n.and_then(|n| self.on_offer(list, stock).nth(n))
            .unwrap_or(FREE)
    }

    fn on_offer(&self, list: Kind, stock: u16) -> impl Iterator<Item = u8> + '_ {
        let words = self.offered.get(list as usize).map_or(&[][..], |w| w);
        let things = words.iter().enumerate();
        things.filter_map(move |(thing, word)| (word & stock != 0).then_some(thing as u8))
    }
}

/// The bit of the stock (`0x800BB258`): the first, the second once the player has thing
/// 4 of the story, the third once he has thing 6, the fourth for argument 3, and the
/// fifth for argument 3 in the room of chapter 0 at area 0, place 4.
pub fn stock(keeping: &Keeping, argument: i16, in_the_first_room: bool) -> u16 {
    if argument == 3 {
        return if in_the_first_room { 16 } else { 8 };
    }
    if keeping.has_story_thing(6) {
        4
    } else if keeping.has_story_thing(4) {
        2
    } else {
        1
    }
}

/// What a pass of the screen did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Passed {
    /// The sounds asked for, in their order.
    pub sounds: Vec<u8>,
    /// A deal was done: the thing, its list and the amount.
    pub bought: Option<(u8, Kind, i32)>,
    pub sold: Option<(u8, Kind, i32)>,
    /// The equip routine took this thing of this kind.
    pub put_on: Option<(u8, i16)>,
    /// The screen leaves in this pass.
    pub ended: bool,
}

/// The record of the screen's task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopScreen {
    /// The state (+0) and the state asked for (+2), -1 for none.
    pub state: i16,
    pub asked: i16,
    /// Bit 1 of the flags at +4: the screen leaves.
    pub leaves: bool,
    /// The panels asked for (+8).
    pub shown: u32,
    /// The kind picked at the blacksmith's (+16): 0 weapon, 1 armour, 2 ornament.
    pub kind: i16,
    /// 0 to buy and 1 to sell (+18).
    pub sells: i16,
    /// The thing (+20) and the amount (+24).
    pub thing: u8,
    pub amount: i32,
    /// The way (+28) and the bit of the stock (+30).
    pub way: i16,
    pub stock: u16,
    /// The cursor record (+40) and the panel that has it (+52).
    pub cursor: Cursor,
    pub panel: i16,
    /// The cursor and the first line of the list, kept (+54, +56).
    pub kept: (i16, i16),
    /// The keeper's line (+58).
    pub line: i16,
    confirm: bool,
    cancel: bool,
    pub openness: i16,
    /// The argument of the script's command (+76): the keeper's voice.
    pub argument: i16,
    /// The refusal that is up (+78, with bit 0 of the flags), 0 for none, and its
    /// passes (+80).
    pub refusal: i16,
    pub refusal_passes: i16,
}

impl ShopScreen {
    /// The screen as its entry leaves it (`0x800BAFFC`): on the first menu, closed.
    pub fn open(way: i16, argument: i16, stock: u16) -> Self {
        let state = if way == BLACKSMITH {
            FIRST_OF_BLACKSMITH
        } else {
            FIRST_OF_SHOP
        };
        let at = state as usize;
        let [columns, rows, lines, flags] = MENU;
        Self {
            state,
            asked: -1,
            leaves: false,
            shown: SHOWN[at],
            kind: 0,
            sells: 0,
            thing: 0,
            amount: 1,
            way,
            stock,
            cursor: Cursor {
                columns,
                rows,
                lines: lines as u16,
                flags: flags as u16,
                at: 0,
                first: 0,
            },
            panel: PANEL_OF[at],
            kept: (0, 0),
            line: LINE_OF[at],
            confirm: false,
            cancel: false,
            openness: 0,
            argument,
            refusal: 0,
            refusal_passes: 0,
        }
    }

    /// The list of the player's that the screen deals in: the items at the shop, the
    /// list of the picked kind at the blacksmith's.
    pub fn list(&self) -> Kind {
        if self.way != BLACKSMITH {
            return Kind::Item;
        }
        match self.kind {
            0 => Kind::Weapon,
            1 => Kind::Armour,
            _ => Kind::Ornament,
        }
    }

    /// The place of the list under the cursor: the cursor's while a list has it, the
    /// kept one behind (`0x800BE1DC`).
    pub fn place(&self) -> usize {
        let (at, first) = match self.state {
            OFFER..=LIST_OF_KIND => (self.cursor.at, self.cursor.first),
            _ => self.kept,
        };
        usize::try_from(i32::from(at) + i32::from(first)).unwrap_or(usize::MAX)
    }

    /// The thing at a place of the list that is up: of the offer while buying, of the
    /// player's list while selling; [`FREE`] for none.
    pub fn thing_at(&self, wares: &Wares, carried: &Carried, place: usize) -> u8 {
        if self.sells != 0 {
            return carried.keeping.number_at(place, self.list());
        }
        let n = i16::try_from(place).unwrap_or(i16::MAX);
        wares.nth(n, self.list(), self.stock)
    }

    /// How many of a thing can be bought (`0x800BE138`, `0x800BE008`): -1 when no pair
    /// of the list takes one more, else what the money pays for, twenty at most and
    /// with what the player has; the one he has on counts.
    pub fn can_buy(&self, wares: &Wares, carried: &Carried, thing: u8) -> i32 {
        let list = self.list();
        let number = i32::from(thing);
        if !carried.keeping.fits(number, 1, list, carried.worn) {
            return -1;
        }
        let has = i32::from(carried.keeping.count(number, list));
        // A division by 0 leaves -1 on the console.
        let paid = carried
            .keeping
            .money()
            .checked_div(wares.price(thing, list));
        let mut most = paid.unwrap_or(-1).min(MOST_OF_A_THING);
        if most + has > MOST_OF_A_THING {
            most = MOST_OF_A_THING - has;
        }
        let on = usize::try_from(self.kind)
            .ok()
            .and_then(|kind| carried.worn.get(kind));
        let is_on = self.way == BLACKSMITH && on == Some(&thing);
        if is_on && most >= 2 && most == MOST_OF_A_THING - has {
            most -= 1;
        }
        most
    }

    /// Whether the circle shows the description of the thing under the cursor
    /// (`0x800BBB20`): in the four states of the lists, for a place with a thing.
    pub fn describes(&self, wares: &Wares, carried: &Carried, pad: &Pad) -> bool {
        (OFFER..=LIST_OF_KIND).contains(&self.state)
            && pad.held & pad::CIRCLE != 0
            && self.thing_at(wares, carried, self.place()) != FREE
    }

    /// One pass of the screen with the pad's words of the pass (`0x800BB3C4`): the
    /// cursor, the buttons with the state's branch, the change of state.
    pub fn pass(&mut self, wares: &Wares, carried: &mut Carried, pad: &Pad) -> Passed {
        let mut passed = Passed::default();
        let amount = (AMOUNT..QUESTION).contains(&self.state);
        if self.refusal == 0 && !amount && self.cursor.step(pad.repeated).sounds {
            passed.sounds.push(SOUND_CURSOR);
        }
        if self.refusal != 0 {
            self.pass_refusal(pad);
        } else {
            if self.openness == OPEN {
                self.confirm = menu::confirmed(pad);
                self.cancel = !self.confirm && menu::cancelled(pad);
            }
            self.act(wares, carried, pad, &mut passed);
            self.confirm = false;
            self.cancel = false;
        }
        if self.asked >= 0 {
            self.enter(wares, carried);
        }
        if self.openness < OPEN {
            self.openness += 1;
        }
        passed.ended = self.leaves;
        passed
    }

    /// A refusal stands until any button goes down or its passes are over
    /// (`0x800BB8A8`); then the state's own line is back.
    fn pass_refusal(&mut self, pad: &Pad) {
        let lines = match self.panel {
            4 => REFUSAL_OF_SHOP,
            _ => REFUSAL_OF_BLACKSMITH,
        };
        self.line = lines[usize::from(self.refusal == NO_ROOM)];
        if self.refusal_passes < MESSAGE_PASSES {
            self.refusal_passes += 1;
        }
        if pad.pressed != 0 || self.refusal_passes >= MESSAGE_PASSES {
            self.refusal = 0;
            self.refusal_passes = 0;
            self.line = LINE_OF[self.state as usize];
        }
    }

    /// The branch of the state (the jump table `0x800BAF74`).
    fn act(&mut self, wares: &Wares, carried: &mut Carried, pad: &Pad, passed: &mut Passed) {
        let sells = usize::from(self.sells == 1);
        match self.state {
            FIRST_OF_SHOP | FIRST_OF_BLACKSMITH => self.first_menu(passed),
            KINDS_TO_BUY | KINDS_TO_SELL => {
                if self.confirm {
                    passed.sounds.push(SOUND_CONFIRM);
                    self.kind = self.cursor.at;
                    self.asked = LISTS_OF_KIND[sells];
                } else if self.cancel {
                    passed.sounds.push(SOUND_CANCEL);
                    self.asked = FIRST_OF_BLACKSMITH;
                }
            }
            OFFER | OFFER_OF_KIND => self.offer(wares, carried, passed),
            ITEMS | LIST_OF_KIND => self.own_list(carried, passed),
            AMOUNT..=11 => self.pick_amount(wares, carried, pad, passed),
            QUESTION..=15 => self.question(wares, carried, passed),
            PUT_ON => {
                if self.confirm && self.cursor.at == YES {
                    passed.sounds.push(SOUND_CONFIRM);
                    carried.equip(i32::from(self.thing), i32::from(self.kind));
                    passed.put_on = Some((self.thing, self.kind));
                    self.asked = PUT_ON_DONE;
                } else if self.cancel || (self.confirm && self.cursor.at == NO) {
                    passed.sounds.push(SOUND_CANCEL);
                    self.asked = PUT_ON_LATER;
                }
            }
            _ => {
                if self.confirm || self.cancel {
                    passed.sounds.push(SOUND_CANCEL);
                    self.asked = LISTS_OF_KIND[sells];
                }
            }
        }
    }

    /// The first menu (`0x800BD378`): a row to buy, a row to sell, a row to leave, and
    /// a cancel that puts the cursor on the last.
    fn first_menu(&mut self, passed: &mut Passed) {
        if self.confirm {
            if self.cursor.at == LEAVE {
                self.leaves = true;
                passed.sounds.push(SOUND_CANCEL);
                return;
            }
            let behind = match self.way {
                BLACKSMITH => Some(BEHIND_BLACKSMITH),
                SHOP => Some(BEHIND_SHOP),
                _ => None,
            };
            let row = usize::try_from(self.cursor.at).ok();
            if let Some(next) = behind.and_then(|behind| behind.get(row?).copied()) {
                self.asked = next;
            }
            passed.sounds.push(SOUND_CONFIRM);
            self.sells = self.cursor.at;
        } else if self.cancel && self.cursor.at != LEAVE {
            self.cursor.at = LEAVE;
            passed.sounds.push(SOUND_CURSOR);
        }
    }

    /// A list of what is on offer (`0x800BD448`, `0x800BDB78`).
    fn offer(&mut self, wares: &Wares, carried: &Carried, passed: &mut Passed) {
        if self.confirm {
            let n = self.cursor.at.wrapping_add(self.cursor.first);
            let thing = wares.nth(n, self.list(), self.stock);
            let most = self.can_buy(wares, carried, thing);
            if most > 0 {
                passed.sounds.push(SOUND_CONFIRM);
                self.ask_amount();
            } else {
                passed.sounds.push(SOUND_REFUSED);
                self.refusal = if most < 0 { NO_ROOM } else { NO_MONEY };
            }
            self.thing = thing;
        } else if self.cancel {
            passed.sounds.push(SOUND_CANCEL);
            self.leave_list();
        }
    }

    /// A list of what the player has (`0x800BD548`, `0x800BDC84`).
    fn own_list(&mut self, carried: &Carried, passed: &mut Passed) {
        if self.confirm {
            let place = self.place();
            if carried.keeping.count_at(place, self.list()) == 0 {
                passed.sounds.push(SOUND_REFUSED);
                return;
            }
            passed.sounds.push(SOUND_CONFIRM);
            self.thing = carried.keeping.number_at(place, self.list());
            self.ask_amount();
        } else if self.cancel {
            passed.sounds.push(SOUND_CANCEL);
            self.leave_list();
        }
    }

    /// The state of the amount behind a list, by buy or sell; any other value of the
    /// word asks for nothing.
    fn ask_amount(&mut self) {
        let first = if self.way == BLACKSMITH { AMOUNT } else { 10 };
        if (0..2).contains(&self.sells) {
            self.asked = first + self.sells;
        }
    }

    /// A cancel on a list: the shop's goes to the first menu and forgets the place, the
    /// blacksmith's to the kinds.
    fn leave_list(&mut self) {
        match self.state {
            OFFER | ITEMS => {
                self.asked = FIRST_OF_SHOP;
                self.kept = (0, 0);
            }
            OFFER_OF_KIND => self.asked = KINDS_TO_BUY,
            _ => self.asked = KINDS_TO_SELL,
        }
    }

    /// The amount (`0x800BD930`, `0x800BE254`): up and down by one, R2 and R1 by ten,
    /// from one to the most.
    fn pick_amount(&mut self, wares: &Wares, carried: &Carried, pad: &Pad, passed: &mut Passed) {
        let buys = matches!(self.state, 8 | 10);
        let most = match buys {
            true => self.can_buy(wares, carried, self.thing),
            false => i32::from(carried.keeping.count_at(self.place(), self.list())),
        };
        let before = self.amount;
        let mut amount = self.amount;
        if pad.repeated & pad::UP != 0 && most > amount {
            amount += 1;
        } else if pad.repeated & pad::DOWN != 0 && amount - 1 > 0 {
            amount -= 1;
        }
        if pad.repeated & pad::R2 != 0 {
            amount += BIG_STEP;
        } else if pad.repeated & pad::R1 != 0 && pad.held & pad::R2 == 0 {
            amount -= BIG_STEP;
        }
        amount = amount.min(most).max(0);
        self.amount = if amount == 0 { 1 } else { amount };
        if self.amount != before {
            passed.sounds.push(SOUND_CURSOR);
        }
        if self.confirm {
            passed.sounds.push(SOUND_CONFIRM);
            self.asked = self.state + 4;
        } else if self.cancel {
            passed.sounds.push(SOUND_CANCEL);
            self.asked = self.list_state();
        }
    }

    /// The state of the list a deal comes from.
    fn list_state(&self) -> i16 {
        let lists = match self.way {
            BLACKSMITH => LISTS_OF_KIND,
            _ => BEHIND_SHOP,
        };
        lists[usize::from(self.sells == 1)]
    }

    /// The question (`0x800BD614`): the yes does the deal, and anything goes back to
    /// the list, but for a thing bought at the blacksmith's that is not the one the
    /// player has on.
    fn question(&mut self, wares: &Wares, carried: &mut Carried, passed: &mut Passed) {
        let yes = self.confirm && self.cursor.at == YES;
        if yes {
            self.deal(wares, carried, passed);
            passed.sounds.push(SOUND_CONFIRM);
        } else if self.cancel || (self.confirm && self.cursor.at == NO) {
            passed.sounds.push(SOUND_CANCEL);
        }
        if !self.confirm && !self.cancel {
            return;
        }
        let on = usize::try_from(self.kind)
            .ok()
            .and_then(|kind| carried.worn.get(kind));
        let asks = self.way == BLACKSMITH
            && yes
            && self.sells == 0
            && (self.thing == FREE || on != Some(&self.thing));
        self.asked = if asks { PUT_ON } else { self.list_state() };
    }

    /// The deal (`0x800BDD50` to `0x800BDE84`): the give routine and the price, or the
    /// take routine and three quarters of it.
    fn deal(&mut self, wares: &Wares, carried: &mut Carried, passed: &mut Passed) {
        let (thing, list, amount) = (self.thing, self.list(), self.amount);
        let number = i32::from(thing);
        match self.sells {
            0 => {
                let worn = carried.worn;
                if carried.keeping.give(number, amount, list, worn) {
                    let sum = wares.price(thing, list).wrapping_mul(amount);
                    carried.keeping.take_money(sum);
                }
                passed.bought = Some((thing, list, amount));
            }
            1 => {
                carried.keeping.take(number, amount, list);
                let sum = wares.worth(thing, list).wrapping_mul(amount);
                carried.keeping.give_money(sum);
                passed.sold = Some((thing, list, amount));
            }
            _ => {}
        }
    }

    /// The change of state (`0x800BB4DC`): the cursor of the new state's panel, the
    /// keeper's line, what the state starts with, the openness from 0 again.
    fn enter(&mut self, wares: &Wares, carried: &Carried) {
        let next = self.asked;
        self.asked = -1;
        let Some(at) = usize::try_from(next).ok().filter(|at| *at < STATES) else {
            return;
        };
        self.panel = PANEL_OF[at];
        if let Some([columns, rows, lines, flags]) = CURSORS[at] {
            self.cursor.columns = columns;
            self.cursor.rows = rows;
            self.cursor.lines = lines as u16;
            self.cursor.flags = flags as u16;
        }
        let list = match next {
            ITEMS => Some(Kind::Item),
            LIST_OF_KIND => Some(self.list()),
            _ => None,
        };
        if let Some(list) = list {
            // A line for every place up to the last in use, four at least.
            let last = carried.keeping.last_place(list) as u16;
            self.cursor.lines = last.max(3) + 1;
        }
        self.line = LINE_OF[at];
        let mut shown = SHOWN[at];
        match next {
            FIRST_OF_SHOP | FIRST_OF_BLACKSMITH => {
                self.cursor.first = 0;
                self.kind = 0;
                self.cursor.at = self.sells;
            }
            OFFER..=LIST_OF_KIND => {
                (self.cursor.at, self.cursor.first) = self.kept;
                if self.sells == 0 {
                    let list = match next {
                        OFFER | ITEMS => Kind::Item,
                        _ => self.list(),
                    };
                    self.cursor.lines = wares.offers(list, self.stock) as u16;
                }
            }
            KINDS_TO_BUY | KINDS_TO_SELL => {
                self.cursor.at = self.kind;
                self.cursor.first = 0;
                self.kept = (0, 0);
            }
            AMOUNT..=11 => {
                self.amount = 1;
                self.kept = (self.cursor.at, self.cursor.first);
                // The list stays on screen under the amount.
                let lists: [u32; 2] = if next < 10 { [10, 11] } else { [4, 5] };
                if let Some(panel) = lists.get(self.sells as usize) {
                    shown |= 1 << panel;
                }
            }
            QUESTION..=PUT_ON => {
                self.cursor.at = NO;
                self.cursor.first = 0;
            }
            _ => {
                self.cursor.at = 0;
                self.cursor.first = 0;
            }
        }
        self.openness = 0;
        self.shown = shown;
        self.state = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Repeat;

    /// The pad's words of a pass in which the buttons went down.
    fn press(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(0, Repeat::of(1), 0);
        pad.read(buttons, Repeat::of(1), 1);
        pad
    }

    /// Wares made up for the tests: three items and two weapons in the first stock,
    /// one more item in the second.
    fn wares() -> Wares {
        Wares {
            offered: [vec![1, 0, 3, 2, 1], vec![1, 1], vec![1], vec![]],
            prices: [vec![15, 0, 50, 70, 10], vec![50, 100], vec![300], vec![]],
        }
    }

    fn rest(screen: &mut ShopScreen, wares: &Wares, carried: &mut Carried) {
        while screen.openness < OPEN {
            screen.pass(wares, carried, &Pad::default());
        }
    }

    /// A press once the screen takes one, and the change of state it brings.
    fn go(screen: &mut ShopScreen, wares: &Wares, carried: &mut Carried, buttons: u16) -> Passed {
        rest(screen, wares, carried);
        screen.pass(wares, carried, &press(buttons))
    }

    #[test]
    fn a_stock_offers_its_things_in_their_order() {
        let wares = wares();
        assert_eq!(wares.offers(Kind::Item, 1), 3);
        assert_eq!(wares.offers(Kind::Item, 2), 2);
        let first: Vec<u8> = (0..4).map(|n| wares.nth(n, Kind::Item, 1)).collect();
        assert_eq!(first, [0, 2, 4, FREE]);
        assert_eq!(wares.nth(1, Kind::Item, 2), 3);
        assert_eq!(
            (wares.price(2, Kind::Item), wares.worth(2, Kind::Item)),
            (50, 37)
        );
        assert_eq!(wares.price(9, Kind::Weapon), 0);
        // The stock follows the things of the story, and argument 3 has its own.
        let mut keeping = Keeping::new();
        assert_eq!(stock(&keeping, 0, false), 1);
        keeping.give_story_thing(4);
        assert_eq!(stock(&keeping, 1, false), 2);
        keeping.give_story_thing(6);
        assert_eq!(stock(&keeping, 1, false), 4);
        assert_eq!(
            (stock(&keeping, 3, false), stock(&keeping, 3, true)),
            (8, 16)
        );
    }

    #[test]
    fn the_shop_sells_items_for_their_price() {
        let wares = wares();
        let mut carried = Carried::new_game();
        carried.keeping.give_money(200);
        let mut screen = ShopScreen::open(SHOP, 1, 1);
        assert_eq!((screen.state, screen.line, screen.shown), (0, 9, SHOWN[0]));
        // Deaf for eight passes, then the first row leads to the offer.
        for _ in 0..OPEN {
            let passed = screen.pass(&wares, &mut carried, &press(pad::CROSS));
            assert_eq!((screen.state, passed.sounds.len()), (FIRST_OF_SHOP, 0));
        }
        let passed = screen.pass(&wares, &mut carried, &press(pad::CROSS));
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        assert_eq!((screen.state, screen.openness), (OFFER, 1));
        assert_eq!((screen.cursor.rows, screen.cursor.lines), (LIST_ROWS, 3));
        // The second row of the offer is item 2, of which the money pays four.
        screen.pass(&wares, &mut carried, &press(pad::DOWN));
        let passed = go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        assert_eq!((screen.state, screen.thing, screen.amount), (10, 2, 1));
        assert_eq!(screen.shown, SHOWN[10] | 1 << 4);
        assert_eq!(screen.can_buy(&wares, &carried, 2), 4);
        // Up by one, R2 by ten up to the most, R1 back down to one.
        rest(&mut screen, &wares, &mut carried);
        let passed = screen.pass(&wares, &mut carried, &press(pad::UP));
        assert_eq!((screen.amount, passed.sounds), (2, vec![SOUND_CURSOR]));
        screen.pass(&wares, &mut carried, &press(pad::R2));
        assert_eq!(screen.amount, 4);
        screen.pass(&wares, &mut carried, &press(pad::R1));
        assert_eq!(screen.amount, 1);
        screen.pass(&wares, &mut carried, &press(pad::UP));
        // The question starts on the no; the yes does the deal.
        screen.pass(&wares, &mut carried, &press(pad::CROSS));
        assert_eq!((screen.state, screen.cursor.at, screen.line), (14, NO, 14));
        screen.pass(&wares, &mut carried, &press(pad::UP));
        let passed = go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!(passed.bought, Some((2, Kind::Item, 2)));
        assert_eq!(passed.sounds, [SOUND_CONFIRM]);
        assert_eq!(carried.keeping.count(2, Kind::Item), 2);
        assert_eq!(carried.keeping.money(), 100);
        // Back on the offer, where the cursor was.
        assert_eq!((screen.state, screen.cursor.at), (OFFER, 1));
    }

    #[test]
    fn the_keeper_refuses_without_money_or_room() {
        let wares = wares();
        let mut carried = Carried::new_game();
        carried.keeping.give_money(14);
        let mut screen = ShopScreen::open(SHOP, 1, 1);
        go(&mut screen, &wares, &mut carried, pad::CROSS);
        let passed = go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!(passed.sounds, [SOUND_REFUSED]);
        assert_eq!((screen.state, screen.refusal), (OFFER, NO_MONEY));
        // The refusal's line stands until a button goes down.
        screen.pass(&wares, &mut carried, &Pad::default());
        assert_eq!(screen.line, REFUSAL_OF_SHOP[0]);
        screen.pass(&wares, &mut carried, &press(pad::SQUARE));
        assert_eq!((screen.refusal, screen.line), (0, LINE_OF[2]));
        // With twenty of the thing nothing more is carried.
        let worn = carried.worn;
        carried.keeping.give_money(1000);
        carried.keeping.give(0, 20, Kind::Item, worn);
        let passed = screen.pass(&wares, &mut carried, &press(pad::CROSS));
        assert_eq!(passed.sounds, [SOUND_REFUSED]);
        assert_eq!(screen.refusal, NO_ROOM);
        screen.pass(&wares, &mut carried, &Pad::default());
        assert_eq!(screen.line, REFUSAL_OF_SHOP[1]);
        // It ends by itself with its 150th pass.
        for _ in 2..MESSAGE_PASSES {
            screen.pass(&wares, &mut carried, &Pad::default());
            assert_eq!(screen.refusal, NO_ROOM);
        }
        screen.pass(&wares, &mut carried, &Pad::default());
        assert_eq!((screen.refusal, screen.refusal_passes), (0, 0));
    }

    #[test]
    fn selling_brings_three_quarters_and_an_empty_row_is_refused() {
        let wares = wares();
        let mut carried = Carried::new_game();
        let worn = carried.worn;
        carried.keeping.give(2, 3, Kind::Item, worn);
        let mut screen = ShopScreen::open(SHOP, 1, 1);
        rest(&mut screen, &wares, &mut carried);
        screen.pass(&wares, &mut carried, &press(pad::DOWN));
        screen.pass(&wares, &mut carried, &press(pad::CROSS));
        assert_eq!((screen.state, screen.sells), (ITEMS, 1));
        // Four lines at least, and the second is empty.
        assert_eq!(screen.cursor.lines, 4);
        screen.pass(&wares, &mut carried, &press(pad::DOWN));
        let passed = go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!((passed.sounds, screen.state), (vec![SOUND_REFUSED], ITEMS));
        screen.pass(&wares, &mut carried, &press(pad::UP));
        screen.pass(&wares, &mut carried, &press(pad::CROSS));
        assert_eq!((screen.state, screen.thing), (11, 2));
        // The most is what he has.
        rest(&mut screen, &wares, &mut carried);
        screen.pass(&wares, &mut carried, &press(pad::R2));
        assert_eq!(screen.amount, 3);
        screen.pass(&wares, &mut carried, &press(pad::CROSS));
        assert_eq!(screen.state, 15);
        screen.pass(&wares, &mut carried, &press(pad::UP));
        let passed = go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!(passed.sold, Some((2, Kind::Item, 3)));
        assert_eq!(carried.keeping.count(2, Kind::Item), 0);
        assert_eq!(carried.keeping.money(), 37 * 3);
        // A cancel on the list goes to the first menu, with the cursor on the row.
        let passed = go(&mut screen, &wares, &mut carried, pad::TRIANGLE);
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
        assert_eq!((screen.state, screen.cursor.at), (FIRST_OF_SHOP, 1));
        // A cancel there puts the cursor on the last row, and its confirm leaves.
        let passed = go(&mut screen, &wares, &mut carried, pad::TRIANGLE);
        assert_eq!(
            (screen.cursor.at, passed.sounds),
            (LEAVE, vec![SOUND_CURSOR])
        );
        let passed = screen.pass(&wares, &mut carried, &press(pad::CROSS));
        assert!(passed.ended);
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
    }

    #[test]
    fn the_blacksmith_asks_whether_to_put_a_bought_thing_on() {
        let wares = wares();
        let mut carried = Carried::new_game();
        carried.keeping.give_money(500);
        let mut screen = ShopScreen::open(BLACKSMITH, 0, 1);
        assert_eq!((screen.state, screen.line), (FIRST_OF_BLACKSMITH, 0));
        // Buy, the first kind, the second weapon of the offer.
        go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!(screen.state, KINDS_TO_BUY);
        go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!((screen.state, screen.cursor.lines), (OFFER_OF_KIND, 2));
        // The one he has on counts: nineteen more of it at most.
        carried.keeping.give_money(5000);
        assert_eq!(screen.can_buy(&wares, &carried, 0), 19);
        assert_eq!(screen.can_buy(&wares, &carried, 1), 20);
        screen.pass(&wares, &mut carried, &press(pad::DOWN));
        go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!((screen.state, screen.thing), (AMOUNT, 1));
        assert_eq!(screen.shown, SHOWN[8] | 1 << 10);
        go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!(screen.state, QUESTION);
        screen.pass(&wares, &mut carried, &press(pad::UP));
        let passed = go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!(passed.bought, Some((1, Kind::Weapon, 1)));
        assert_eq!(carried.keeping.money(), 5400);
        // It is not the one he has on: the question, on the no.
        assert_eq!((screen.state, screen.cursor.at), (PUT_ON, NO));
        screen.pass(&wares, &mut carried, &press(pad::UP));
        let passed = go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!(passed.put_on, Some((1, 0)));
        assert_eq!((carried.worn[0], screen.state), (1, PUT_ON_DONE));
        assert_eq!(carried.keeping.count(0, Kind::Weapon), 1);
        // Any of the two buttons goes back to the offer.
        let passed = go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!(passed.sounds, [SOUND_CANCEL]);
        assert_eq!((screen.state, screen.cursor.at), (OFFER_OF_KIND, 1));
        // Buying the one he has on asks nothing; a cancel leads back to the kinds.
        go(&mut screen, &wares, &mut carried, pad::CROSS);
        go(&mut screen, &wares, &mut carried, pad::CROSS);
        screen.pass(&wares, &mut carried, &press(pad::UP));
        go(&mut screen, &wares, &mut carried, pad::CROSS);
        assert_eq!(screen.state, OFFER_OF_KIND);
        go(&mut screen, &wares, &mut carried, pad::TRIANGLE);
        assert_eq!((screen.state, screen.kept), (KINDS_TO_BUY, (0, 0)));
        go(&mut screen, &wares, &mut carried, pad::TRIANGLE);
        assert_eq!(screen.state, FIRST_OF_BLACKSMITH);
    }
}
