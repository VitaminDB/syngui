//! «Слово под пальцем»: удержание (на телефоне) или правый щелчок (на
//! компьютере) по любому `Text` отдаёт приложению слово и предложение вокруг
//! него — например, для перевода в контексте. Приложение ставит обработчик
//! [`set_word_lookup`]; обработчик возвращает `true`, если взял слово (тогда
//! событие считается обработанным). Отключить у конкретного текста —
//! `Text::word_lookup(false)`.

use std::sync::{Arc, RwLock};

/// Слово, на которое нажали.
#[derive(Debug, Clone, PartialEq)]
pub struct WordHit {
    /// Слово без окружающих знаков.
    pub word: String,
    /// Предложение, в котором оно стоит (по . ! ? и переводам строк).
    pub sentence: String,
    /// Весь текст виджета.
    pub text: String,
    /// Точка нажатия (координаты окна).
    pub position: crate::core::Point,
}

type Hook = Arc<dyn Fn(&WordHit) -> bool + Send + Sync>;

static HOOK: RwLock<Option<Hook>> = RwLock::new(None);

/// Поставить обработчик слова под пальцем (`None` — снять).
pub fn set_word_lookup(f: Option<impl Fn(&WordHit) -> bool + Send + Sync + 'static>) {
    *HOOK.write().unwrap_or_else(|e| e.into_inner()) = f.map(|f| Arc::new(f) as Hook);
}

pub(crate) fn enabled() -> bool {
    HOOK.read().unwrap_or_else(|e| e.into_inner()).is_some()
}

pub(crate) fn dispatch(hit: &WordHit) -> bool {
    let h = HOOK.read().unwrap_or_else(|e| e.into_inner()).clone();
    h.is_some_and(|h| h(hit))
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '’' | 'ʼ' | '-')
}

/// Слово и предложение вокруг байта `at` (`None` — не на слове).
pub fn word_at(text: &str, at: usize, position: crate::core::Point) -> Option<WordHit> {
    let at = at.min(text.len());
    // Нажатие сразу за словом (между буквой и пробелом) — тоже это слово.
    let mut idx = text.char_indices().map(|(b, _)| b).filter(|&b| b <= at).last().unwrap_or(0);
    if !text[idx..].chars().next().is_some_and(is_word_char) {
        let prev = text[..idx].char_indices().last().map(|(b, _)| b);
        match prev {
            Some(p) if text[p..].chars().next().is_some_and(is_word_char) => idx = p,
            _ => return None,
        }
    }
    let start = text[..idx].char_indices().rev().take_while(|&(_, c)| is_word_char(c)).last().map_or(idx, |(b, _)| b);
    let end = text[idx..].char_indices().find(|&(_, c)| !is_word_char(c)).map_or(text.len(), |(b, _)| idx + b);
    let word = text[start..end].trim_matches(|c| matches!(c, '\'' | '’' | 'ʼ' | '-')).to_string();
    if !word.chars().any(|c| c.is_alphabetic()) {
        return None;
    }
    let stop = |c: char| matches!(c, '.' | '!' | '?' | '\n' | '…');
    let s0 = text[..start].char_indices().rev().find(|&(_, c)| stop(c)).map_or(0, |(b, c)| b + c.len_utf8());
    let s1 = text[end..].char_indices().find(|&(_, c)| stop(c)).map_or(text.len(), |(b, c)| end + b + c.len_utf8());
    let sentence = text[s0..s1].trim().to_string();
    Some(WordHit { word, sentence, text: text.to_string(), position })
}

/// Позиции символов текста при переносе по ширине (как при отрисовке):
/// (байт, строка, x начала, x конца).
pub(crate) fn char_positions(
    text: &str,
    width: f32,
    font_size: f32,
    weight: u16,
    family: Option<&str>,
    tm: &dyn crate::widget::context::TextMeasure,
) -> Vec<(usize, usize, f32, f32)> {
    let mut out = Vec::with_capacity(text.len());
    let mut line = 0usize;
    let mut base = 0usize;
    let mut buf = [0u8; 4];
    for seg in text.split('\n') {
        // Слова сегмента: (начало, конец) байтов, ширины символов.
        let mut x = 0.0f32;
        let chars: Vec<(usize, char, f32)> = seg
            .char_indices()
            .map(|(b, c)| (base + b, c, tm.measure_text_width_weight(c.encode_utf8(&mut buf), font_size, 1, weight, family)))
            .collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].1 == ' ' {
                out.push((chars[i].0, line, x, x + chars[i].2));
                x += chars[i].2;
                i += 1;
                continue;
            }
            let j = chars[i..].iter().position(|c| c.1 == ' ').map_or(chars.len(), |p| i + p);
            let w: f32 = chars[i..j].iter().map(|c| c.2).sum();
            if x + w > width && x > 0.0 {
                line += 1;
                x = 0.0;
            }
            for c in &chars[i..j] {
                if x + c.2 > width && x > 0.0 && w > width {
                    line += 1;
                    x = 0.0;
                }
                out.push((c.0, line, x, x + c.2));
                x += c.2;
            }
            i = j;
        }
        base += seg.len() + 1;
        line += 1;
    }
    out
}

/// Байт символа под точкой (`x`, номер строки).
pub(crate) fn byte_at(positions: &[(usize, usize, f32, f32)], line: usize, x: f32) -> Option<usize> {
    let on_line: Vec<&(usize, usize, f32, f32)> = positions.iter().filter(|p| p.1 == line).collect();
    on_line.iter().find(|p| x >= p.2 && x < p.3).or_else(|| on_line.iter().min_by(|a, b| (a.2 - x).abs().total_cmp(&(b.2 - x).abs()))).map(|p| p.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Point;

    #[test]
    fn finds_word_and_sentence() {
        let t = "I went home. She has a cat, doesn't she? Yes.";
        let at = t.find("cat").unwrap() + 1;
        let h = word_at(t, at, Point::zero()).unwrap();
        assert_eq!(h.word, "cat");
        assert_eq!(h.sentence, "She has a cat, doesn't she?");
        let h = word_at(t, t.find("doesn't").unwrap() + 5, Point::zero()).unwrap();
        assert_eq!(h.word, "doesn't");
        assert!(word_at(t, t.find(',').unwrap() + 1, Point::zero()).is_none(), "пробел после запятой — не слово");
    }

    #[test]
    fn unicode_word() {
        let t = "Мен қазақша сөйлеймін.";
        let h = word_at(t, t.find("қазақша").unwrap() + 2, Point::zero()).unwrap();
        assert_eq!(h.word, "қазақша");
    }

    struct Mono;
    impl crate::widget::context::TextMeasure for Mono {
        fn measure_text_width(&self, _t: &str, _f: f32, n: usize) -> f32 {
            n as f32 * 10.0
        }
        fn hit_test_char(&self, _t: &str, _f: f32, x: f32) -> usize {
            (x / 10.0) as usize
        }
    }

    #[test]
    fn wrapped_positions() {
        // Ширина 60 = 6 символов: "aaa bbb ccc" → строки «aaa », «bbb », «ccc».
        let t = "aaa bbb ccc";
        let p = char_positions(t, 60.0, 14.0, 400, None, &Mono);
        let b = byte_at(&p, 1, 15.0).unwrap();
        assert_eq!(&t[b..b + 1], "b");
        let b = byte_at(&p, 2, 5.0).unwrap();
        assert_eq!(&t[b..b + 1], "c");
    }

    #[test]
    fn long_press_on_text_widget() {
        use crate::input::Event;
        use std::sync::Mutex;
        let got: Arc<Mutex<Vec<WordHit>>> = Arc::new(Mutex::new(Vec::new()));
        let g = got.clone();
        set_word_lookup(Some(move |h: &WordHit| {
            g.lock().unwrap().push(h.clone());
            true
        }));
        let text = crate::widget::Text::new("The cat sat on the mat. It was happy.");
        let mut h = crate::testing::TestHarness::new(Box::new(text));
        h.layout(400.0, 100.0);
        // Первая строка, чуть правее начала — слово «The».
        let r = h.send_event(&Event::LongPress { position: Point::new(4.0, 6.0) });
        set_word_lookup(None::<fn(&WordHit) -> bool>);
        assert!(r.is_handled(), "удержание по тексту обработано");
        let v = got.lock().unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].word, "The");
        assert_eq!(v[0].sentence, "The cat sat on the mat.");
    }
}
