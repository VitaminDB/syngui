//! Разбор PPD-описания принтера (то, что CUPS отдаёт по
//! `http://сервер:631/printers/ИМЯ.ppd`): настройки с вариантами, размеры
//! бумаги в пунктах, непечатаемые поля, цветной ли принтер.

use std::collections::HashMap;

/// Вариант настройки: ключ для `lp -o КЛЮЧ=ВАРИАНТ` и подпись.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub key: String,
    pub label: String,
}

/// Настройка принтера (`*OpenUI`).
#[derive(Debug, Clone, PartialEq)]
pub struct PpdOption {
    pub key: String,
    pub label: String,
    /// Группа (`*OpenGroup`), пусто — без группы.
    pub group: String,
    pub choices: Vec<Choice>,
    pub default: String,
}

impl PpdOption {
    pub fn choice(&self, key: &str) -> Option<&Choice> {
        self.choices.iter().find(|c| c.key == key)
    }
}

/// Описание принтера.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ppd {
    pub options: Vec<PpdOption>,
    /// Размер бумаги по ключу `PageSize`, пункты (ширина, высота).
    pub dimensions: HashMap<String, (f32, f32)>,
    /// Печатаемая область по ключу `PageSize`, пункты (лево, низ, право, верх) от левого нижнего угла.
    pub imageable: HashMap<String, (f32, f32, f32, f32)>,
    pub color: bool,
    pub model: String,
}

impl Ppd {
    pub fn option(&self, key: &str) -> Option<&PpdOption> {
        self.options.iter().find(|o| o.key == key)
    }

    /// Непечатаемые поля бумаги, мм (лево, верх, право, низ).
    pub fn hw_margins_mm(&self, page: &str) -> Option<[f32; 4]> {
        let (w, h) = *self.dimensions.get(page)?;
        let (l, b, r, t) = *self.imageable.get(page)?;
        let mm = |pt: f32| (pt * 25.4 / 72.0).max(0.0);
        Some([mm(l), mm(h - t), mm(w - r), mm(b)])
    }
}

/// `*Ключ Вариант/Подпись: "значение"` → (ключ, вариант, подпись, значение).
fn split_line(line: &str) -> Option<(&str, &str, &str, &str)> {
    let line = line.strip_prefix('*')?;
    let (head, value) = match line.find(':') {
        Some(i) => (&line[..i], line[i + 1..].trim()),
        None => (line, ""),
    };
    let (key, rest) = match head.find(char::is_whitespace) {
        Some(i) => (&head[..i], head[i..].trim()),
        None => (head, ""),
    };
    let (opt, label) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i + 1..].trim()),
        None => (rest, ""),
    };
    Some((key, opt.trim(), label, value))
}

fn unquote(v: &str) -> &str {
    let v = v.trim();
    let v = v.strip_prefix('"').unwrap_or(v);
    v.split('"').next().unwrap_or(v)
}

fn nums(v: &str) -> Vec<f32> {
    unquote(v).split_whitespace().filter_map(|s| s.parse().ok()).collect()
}

/// Разобрать текст PPD.
pub fn parse(text: &str) -> Ppd {
    let mut ppd = Ppd::default();
    let mut group = String::new();
    let mut current: Option<usize> = None;
    let mut defaults: HashMap<String, String> = HashMap::new();
    // Значение может тянуться на несколько строк: пропускаем до закрывающей кавычки.
    let mut in_value = false;
    for raw in text.lines() {
        if in_value {
            if raw.contains('"') {
                in_value = false;
            }
            continue;
        }
        if !raw.starts_with('*') || raw.starts_with("*%") {
            continue;
        }
        let Some((key, opt, label, value)) = split_line(raw) else { continue };
        if value.starts_with('"') && value[1..].find('"').is_none() {
            in_value = true;
        }
        match key {
            "OpenGroup" => {
                let v = value.split('/').nth(1).unwrap_or(value);
                group = v.trim().to_string();
            }
            "CloseGroup" => group.clear(),
            "OpenUI" => {
                let name = opt.trim_start_matches('*');
                // `PageRegion` дублирует `PageSize`.
                if name == "PageRegion" {
                    current = None;
                    continue;
                }
                ppd.options.push(PpdOption {
                    key: name.to_string(),
                    label: if label.is_empty() { name.to_string() } else { label.to_string() },
                    group: group.clone(),
                    choices: Vec::new(),
                    default: String::new(),
                });
                current = Some(ppd.options.len() - 1);
            }
            "CloseUI" => current = None,
            "ColorDevice" => ppd.color = value.eq_ignore_ascii_case("true"),
            "ModelName" | "NickName" if ppd.model.is_empty() => ppd.model = unquote(value).to_string(),
            "PaperDimension" => {
                if let [w, h] = nums(value)[..] {
                    ppd.dimensions.insert(opt.to_string(), (w, h));
                }
            }
            "ImageableArea" => {
                if let [l, b, r, t] = nums(value)[..] {
                    ppd.imageable.insert(opt.to_string(), (l, b, r, t));
                }
            }
            k if k.starts_with("Default") => {
                defaults.insert(k["Default".len()..].to_string(), value.trim().to_string());
            }
            k => {
                if let Some(i) = current {
                    let o = &mut ppd.options[i];
                    if o.key == k && !opt.is_empty() {
                        o.choices.push(Choice {
                            key: opt.to_string(),
                            label: if label.is_empty() { opt.to_string() } else { label.to_string() },
                        });
                    }
                }
            }
        }
    }
    // Варианты PageSize без явного `*PaperDimension` — из `<</PageSize[w h]>>`.
    for line in text.lines().filter(|l| l.starts_with("*PageSize ")) {
        if let Some((_, opt, _, value)) = split_line(line) {
            if ppd.dimensions.contains_key(opt) {
                continue;
            }
            if let Some(i) = value.find("PageSize") {
                let tail = &value[i + 8..];
                let tail = tail.trim_start_matches(|c: char| c == '[' || c.is_whitespace());
                let n: Vec<f32> = tail.split(|c: char| c == ']' || c.is_whitespace()).filter_map(|s| s.parse().ok()).take(2).collect();
                if let [w, h] = n[..] {
                    ppd.dimensions.insert(opt.to_string(), (w, h));
                }
            }
        }
    }
    ppd.options.retain(|o| !o.choices.is_empty());
    for o in &mut ppd.options {
        o.default = defaults
            .get(&o.key)
            .filter(|d| o.choices.iter().any(|c| &c.key == *d))
            .cloned()
            .unwrap_or_else(|| o.choices[0].key.clone());
    }
    ppd
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"*PPD-Adobe: "4.3"
*ModelName: "Samsung ML-2160"
*ColorDevice: False
*OpenGroup: General/General
*OpenUI *PageSize/Media Size: PickOne
*DefaultPageSize: A4
*PageSize Letter/US Letter: "<</PageSize[612 792]>>setpagedevice"
*PageSize A4/A4: "<</PageSize[595.28 841.89]>>setpagedevice"
*CloseUI: *PageSize
*OpenUI *PageRegion: PickOne
*PageRegion A4: "x"
*CloseUI: *PageRegion
*ImageableArea A4/A4: "12.5 12.5 582.5 829.5"
*PaperDimension Letter: "612 792"
*CloseGroup: General
*OpenUI *Duplex/2-Sided: PickOne
*DefaultDuplex: None
*Duplex None/Off: "
<</Duplex false>>setpagedevice"
*Duplex DuplexNoTumble/Long Edge: "<</Duplex true>>setpagedevice"
*CloseUI: *Duplex
"#;

    #[test]
    fn parses_options_sizes_margins() {
        let p = parse(SAMPLE);
        assert!(!p.color);
        assert_eq!(p.model, "Samsung ML-2160");
        let ps = p.option("PageSize").unwrap();
        assert_eq!(ps.default, "A4");
        assert_eq!(ps.group, "General");
        assert_eq!(ps.choices.len(), 2);
        assert!(p.option("PageRegion").is_none());
        assert_eq!(p.dimensions["Letter"], (612.0, 792.0));
        assert!((p.dimensions["A4"].0 - 595.28).abs() < 0.01, "из PageSize[...]");
        let m = p.hw_margins_mm("A4").unwrap();
        assert!((m[0] - 4.41).abs() < 0.05 && (m[1] - 4.40).abs() < 0.1);
        let d = p.option("Duplex").unwrap();
        assert_eq!(d.default, "None");
        assert_eq!(d.choices[1].label, "Long Edge", "многострочное значение не сбивает разбор");
    }
}
