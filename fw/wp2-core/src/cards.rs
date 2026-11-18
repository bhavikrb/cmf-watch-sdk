//! The stock card list left of the watch face (ux2sys property 0x2F, docs/recon.md "Widget cards"): 8 entries of
//! 12 bytes, then `cycle_scrolling_sw`. Type 1 = widget card (`+2` count, `+3` {widget id, param} pairs), type 2 =
//! view card (`+2` u16 view id). The stock stops at the first entry that is neither, so entries stay packed.
//! The loader's Apps menu edits it here and stores it as the phone's 0x906D set does.

pub const BYTES: usize = 0x62;
const ENTRY: usize = 12;
const ENTRIES: usize = 8;
const WIDGETS: u8 = 1;
const VIEW: u8 = 2;
/// the stub's widget (wgt_reg slot 0x40); param 0 = Apps, slot + 1 = that app
pub const APP_WIDGET: u8 = 0x40;
/// param bit (stub 7, card.rs): the small tile (class 1, 166×166) instead of the wide one (class 3, 344×166)
pub const SMALL: u8 = 0x80;
/// a card is a 2×2 grid of 166 px cells: a wide tile takes a row, two small ones share one
const ROWS: usize = 2;
/// the stock layout reads at most four widgets of a card
const PER_CARD: usize = 4;
/// view ids 0xF8 + slot: the app in that slot as a whole page (stub view table, `wp2apps firmware`)
pub const PAGE_VIEW: u16 = 0xF8;

pub type Cards = [u8; BYTES];

#[derive(Debug, PartialEq)]
pub struct Full;

pub fn page_slot(view_id: u32) -> Option<u8> {
    let s = view_id.wrapping_sub(PAGE_VIEW as u32);
    (s < crate::layout::SLOTS as u32).then_some(s as u8)
}

fn len(c: &Cards) -> usize {
    (0..ENTRIES).take_while(|&i| matches!(c[i * ENTRY], WIDGETS | VIEW)).count()
}

fn entry(c: &mut Cards, i: usize) -> &mut [u8] {
    &mut c[i * ENTRY..(i + 1) * ENTRY]
}

fn remove(c: &mut Cards, i: usize) {
    let n = len(c);
    c.copy_within((i + 1) * ENTRY..n * ENTRY, i * ENTRY);
    entry(c, n - 1).fill(0);
}

fn view(c: &Cards, i: usize) -> Option<u16> {
    (c[i * ENTRY] == VIEW).then(|| u16::from_le_bytes([c[i * ENTRY + 2], c[i * ENTRY + 3]]))
}

fn app_widget_at(c: &Cards, i: usize, slot: u8) -> Option<usize> {
    let e = &c[i * ENTRY..(i + 1) * ENTRY];
    if e[0] != WIDGETS {
        return None;
    }
    (0..usize::from(e[2]).min(PER_CARD)).find(|&k| e[3 + 2 * k] == APP_WIDGET && e[4 + 2 * k] & !SMALL == slot + 1)
}

// rows a card of app widgets takes with one more tile; the wide ones stand first (toggle_widget)
fn rows_with(e: &[u8], small: bool) -> usize {
    let n = usize::from(e[2]);
    let smalls = (0..n).filter(|&k| e[4 + 2 * k] & SMALL != 0).count() + usize::from(small);
    n + 1 - smalls + smalls.div_ceil(2)
}

pub fn has_widget(c: &Cards, slot: u8) -> bool {
    (0..len(c)).any(|i| app_widget_at(c, i, slot).is_some())
}

pub fn has_page(c: &Cards, slot: u8) -> bool {
    (0..len(c)).any(|i| view(c, i) == Some(PAGE_VIEW + u16::from(slot)))
}

/// Adds the app's widget, `small` or wide (to the last card that holds only app widgets and has room, else a new card
/// at the end), or removes it, whichever size it has (and its card once empty). Ok(true) = now shown.
pub fn toggle_widget(c: &mut Cards, slot: u8, small: bool) -> Result<bool, Full> {
    for i in 0..len(c) {
        if let Some(k) = app_widget_at(c, i, slot) {
            let e = entry(c, i);
            let n = usize::from(e[2]);
            e.copy_within(3 + 2 * (k + 1)..3 + 2 * n, 3 + 2 * k);
            e[1 + 2 * n..3 + 2 * n].fill(0);
            e[2] -= 1;
            if e[2] == 0 {
                remove(c, i);
            }
            return Ok(false);
        }
    }
    let n = len(c);
    let last = (0..n).rev().find(|&i| {
        let e = &c[i * ENTRY..(i + 1) * ENTRY];
        e[0] == WIDGETS
            && usize::from(e[2]) < PER_CARD
            && (0..usize::from(e[2])).all(|k| e[3 + 2 * k] == APP_WIDGET)
            && rows_with(e, small) <= ROWS
    });
    let i = match last {
        Some(i) => i,
        None if n < ENTRIES => {
            entry(c, n)[0] = WIDGETS;
            n
        }
        None => return Err(Full),
    };
    let e = entry(c, i);
    let n = usize::from(e[2]);
    // The stock layout puts a tile right of the previous one while the row has room and never checks that it fits:
    // a wide tile after a single small one would hang out of the card. So wide tiles go before the small ones.
    let k = if small { n } else { (0..n).find(|&k| e[4 + 2 * k] & SMALL != 0).unwrap_or(n) };
    e.copy_within(3 + 2 * k..3 + 2 * n, 3 + 2 * (k + 1));
    (e[3 + 2 * k], e[4 + 2 * k]) = (APP_WIDGET, slot + 1 | if small { SMALL } else { 0 });
    e[2] += 1;
    Ok(true)
}

/// Makes the app the page next to the face (first card; another app's page goes, one loader runs one app), or
/// removes it. Ok(true) = now shown.
pub fn toggle_page(c: &mut Cards, slot: u8) -> Result<bool, Full> {
    let was = has_page(c, slot);
    while let Some(i) = (0..len(c)).find(|&i| view(c, i).is_some_and(|v| page_slot(v.into()).is_some())) {
        remove(c, i);
    }
    if was {
        return Ok(false);
    }
    let n = len(c);
    if n == ENTRIES {
        return Err(Full);
    }
    c.copy_within(0..n * ENTRY, ENTRY);
    entry(c, 0).fill(0);
    let v = (PAGE_VIEW + u16::from(slot)).to_le_bytes();
    entry(c, 0)[..4].copy_from_slice(&[VIEW, 0, v[0], v[1]]);
    Ok(true)
}

/// Drops the app's widget and page (the app is deleted).
pub fn forget(c: &mut Cards, slot: u8) {
    while has_widget(c, slot) {
        let _ = toggle_widget(c, slot, false);
    }
    if has_page(c, slot) {
        let _ = toggle_page(c, slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards(entries: &[&[u8]]) -> Cards {
        let mut c = [0u8; BYTES];
        for (i, e) in entries.iter().enumerate() {
            c[i * ENTRY..i * ENTRY + e.len()].copy_from_slice(e);
        }
        c[0x60] = 1;
        c
    }

    #[test]
    fn widgets_fill_cards_two_at_a_time() {
        let mut c = cards(&[&[VIEW, 0, 0xB2, 0], &[WIDGETS, 0, 1, 0x19, 0]]);
        assert_eq!(toggle_widget(&mut c, 2, false), Ok(true));
        assert_eq!(&c[24..29], &[WIDGETS, 0, 1, 0x40, 3], "a stock widget card is not shared");
        assert_eq!(toggle_widget(&mut c, 0, false), Ok(true));
        assert_eq!(&c[24..31], &[WIDGETS, 0, 2, 0x40, 3, 0x40, 1]);
        assert_eq!(toggle_widget(&mut c, 5, false), Ok(true));
        assert_eq!(&c[36..41], &[WIDGETS, 0, 1, 0x40, 6]);
        assert!(has_widget(&c, 0) && has_widget(&c, 5) && !has_widget(&c, 1));
        assert_eq!(toggle_widget(&mut c, 2, false), Ok(false));
        assert_eq!(&c[24..31], &[WIDGETS, 0, 1, 0x40, 1, 0, 0]);
        assert_eq!(toggle_widget(&mut c, 0, false), Ok(false));
        assert_eq!(&c[24..29], &[WIDGETS, 0, 1, 0x40, 6], "empty card removed, the rest packed");
        assert!(c[36..0x60].iter().all(|&b| b == 0));
        assert_eq!(c[0x60], 1, "wrap byte kept");
    }

    #[test]
    fn small_tiles_share_a_row() {
        let mut c = cards(&[&[VIEW, 0, 0xB2, 0]]);
        for slot in 0..4 {
            assert_eq!(toggle_widget(&mut c, slot, true), Ok(true));
        }
        assert_eq!(&c[12..23], &[WIDGETS, 0, 4, 0x40, 0x81, 0x40, 0x82, 0x40, 0x83, 0x40, 0x84], "two rows of two");
        assert_eq!(toggle_widget(&mut c, 4, true), Ok(true));
        assert_eq!(&c[24..29], &[WIDGETS, 0, 1, 0x40, 0x85], "a full card is not touched");
        // a wide tile goes before the small ones: after a single small one the stock layout would hang it out
        assert_eq!(toggle_widget(&mut c, 5, false), Ok(true));
        assert_eq!(&c[24..31], &[WIDGETS, 0, 2, 0x40, 6, 0x40, 0x85]);
        assert_eq!(toggle_widget(&mut c, 6, true), Ok(true));
        assert_eq!(&c[24..33], &[WIDGETS, 0, 3, 0x40, 6, 0x40, 0x85, 0x40, 0x87], "a wide row, then a row of two");
        assert_eq!(toggle_widget(&mut c, 7, true), Ok(true));
        assert_eq!(&c[36..41], &[WIDGETS, 0, 1, 0x40, 0x88], "a third row does not fit: a new card");
        // either size is found and removed, whatever the caller says it is
        assert!(has_widget(&c, 4) && has_widget(&c, 5) && !has_widget(&cards(&[]), 4));
        assert_eq!(toggle_widget(&mut c, 4, false), Ok(false));
        assert_eq!(&c[24..33], &[WIDGETS, 0, 2, 0x40, 6, 0x40, 0x87, 0, 0]);
        assert_eq!(toggle_widget(&mut c, 1, true), Ok(false));
        assert_eq!(&c[12..23], &[WIDGETS, 0, 3, 0x40, 0x81, 0x40, 0x83, 0x40, 0x84, 0, 0]);
        // two wide tiles fill a card as before; a small one then takes a new card
        let mut c = cards(&[&[WIDGETS, 0, 2, 0x40, 1, 0x40, 2]]);
        assert_eq!(toggle_widget(&mut c, 2, true), Ok(true));
        assert_eq!(&c[12..17], &[WIDGETS, 0, 1, 0x40, 0x83]);
        forget(&mut c, 2);
        assert_eq!(len(&c), 1);
    }

    #[test]
    fn one_page_app_first() {
        let mut c = cards(&[&[WIDGETS, 0, 1, 0x40, 0]]);
        assert_eq!(toggle_page(&mut c, 3), Ok(true));
        assert_eq!(&c[..4], &[VIEW, 0, 0xFB, 0]);
        assert_eq!(&c[12..17], &[WIDGETS, 0, 1, 0x40, 0]);
        assert_eq!(toggle_page(&mut c, 1), Ok(true));
        assert_eq!(&c[..4], &[VIEW, 0, 0xF9, 0], "the other app's page goes");
        assert_eq!(len(&c), 2);
        assert!(has_page(&c, 1) && !has_page(&c, 3));
        assert_eq!(toggle_page(&mut c, 1), Ok(false));
        assert_eq!(&c[..5], &[WIDGETS, 0, 1, 0x40, 0]);
        assert_eq!(len(&c), 1);
    }

    #[test]
    fn full_list_and_forget() {
        let mut c = cards(&[&[VIEW, 0, 1, 0][..]; 8]);
        assert_eq!(toggle_page(&mut c, 0), Err(Full));
        assert_eq!(toggle_widget(&mut c, 0, false), Err(Full));
        assert_eq!(toggle_widget(&mut c, 0, true), Err(Full));
        let mut c = cards(&[&[VIEW, 0, 0xF8, 0], &[WIDGETS, 0, 2, 0x40, 1, 0x40, 2]]);
        forget(&mut c, 0);
        assert_eq!(&c[..5], &[WIDGETS, 0, 1, 0x40, 2]);
        assert_eq!(len(&c), 1);
    }

    #[test]
    fn page_views() {
        assert_eq!(page_slot(0xF8), Some(0));
        assert_eq!(page_slot(0xFF), Some(7));
        assert_eq!(page_slot(0xF0), None);
        assert_eq!(page_slot(0x100), None);
    }
}
