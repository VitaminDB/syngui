#[derive(Clone, Debug)]
pub struct TileProvider {
    pub id: u8,
    pub name: &'static str,
    url_template: &'static str,
    subdomains: &'static [&'static str],
    pub max_zoom: u8,
    pub attribution: &'static str,
    /// Цветовой фильтр плиток при выводе (только плитки, без линий, меток и подписи). Пустой — без фильтра.
    pub filter: &'static [FilterEffect],
}

use crate::effects::FilterEffect;

const OSM_LIGHT_FILTER: &[FilterEffect] =
    &[FilterEffect::Grayscale(0.6), FilterEffect::Brightness(1.06), FilterEffect::Contrast(0.92)];
const OSM_DARK_FILTER: &[FilterEffect] = &[
    FilterEffect::Invert(1.0),
    FilterEffect::HueRotate(180.0),
    FilterEffect::Brightness(0.9),
    FilterEffect::Contrast(0.85),
    FilterEffect::Saturate(0.6),
];

impl TileProvider {
    pub fn osm() -> Self {
        Self {
            id: 0,
            name: "OpenStreetMap",
            // без поддоменов a/b/c: OSM просит обращаться к tile.openstreetmap.org (HTTP/2, один адрес)
            url_template: "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
            subdomains: &[""],
            max_zoom: 19,
            attribution: "\u{00a9} OpenStreetMap contributors",
            filter: &[],
        }
    }

    /// OpenStreetMap в светлых приглушённых тонах: те же плитки (и тот же кэш, `id` 0), что у [`Self::osm`],
    /// под цветовым фильтром при выводе.
    pub fn osm_light() -> Self {
        Self {
            name: "OpenStreetMap Light",
            filter: OSM_LIGHT_FILTER,
            ..Self::osm()
        }
    }

    /// OpenStreetMap в тёмных тонах: плитки [`Self::osm`] под фильтром (инверсия с поворотом оттенка — вода
    /// остаётся синей, парки зелёными).
    pub fn osm_dark() -> Self {
        Self {
            name: "OpenStreetMap Dark",
            filter: OSM_DARK_FILTER,
            ..Self::osm()
        }
    }

    /// Плитки CARTO без ключа приходят с водяным знаком «API KEY REQUIRED» — теперь это [`Self::osm_light`].
    #[deprecated(note = "CARTO требует ключ; используйте TileProvider::osm_light()")]
    pub fn carto_light() -> Self {
        Self::osm_light()
    }

    /// Плитки CARTO без ключа приходят с водяным знаком «API KEY REQUIRED» — теперь это [`Self::osm_dark`].
    #[deprecated(note = "CARTO требует ключ; используйте TileProvider::osm_dark()")]
    pub fn carto_dark() -> Self {
        Self::osm_dark()
    }

    /// Плитки CARTO без ключа приходят с водяным знаком «API KEY REQUIRED» — теперь это [`Self::osm_light`].
    #[deprecated(note = "CARTO требует ключ; используйте TileProvider::osm_light()")]
    pub fn carto_voyager() -> Self {
        Self::osm_light()
    }

    pub fn esri_satellite() -> Self {
        Self {
            id: 4,
            name: "ESRI Satellite",
            url_template: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
            subdomains: &[""],
            max_zoom: 18,
            attribution: "\u{00a9} Esri, DigitalGlobe, GeoEye",
            filter: &[],
        }
    }

    /// Свой источник растровых плиток: шаблон с `{z}`, `{x}`, `{y}` и необязательным `{s}` (поддомен из
    /// `subdomains`). `id` — ключ кэша плиток: у разных источников он должен различаться (встроенные — 0…4).
    pub fn custom(
        id: u8,
        name: &'static str,
        url_template: &'static str,
        subdomains: &'static [&'static str],
        max_zoom: u8,
        attribution: &'static str,
    ) -> Self {
        Self {
            id,
            name,
            url_template,
            subdomains: if subdomains.is_empty() {
                &[""]
            } else {
                subdomains
            },
            max_zoom,
            attribution,
            filter: &[],
        }
    }

    /// Тот же источник с цветовым фильтром плиток (см. [`TileProvider::filter`]).
    pub fn with_filter(mut self, filter: &'static [FilterEffect]) -> Self {
        self.filter = filter;
        self
    }

    pub fn tile_url(&self, x: u32, y: u32, z: u8) -> String {
        let s_idx = ((x + y) as usize) % self.subdomains.len();
        let s = self.subdomains[s_idx];
        self.url_template
            .replace("{s}", s)
            .replace("{z}", &z.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_and_dark_are_filtered_osm() {
        let osm = TileProvider::osm();
        for p in [TileProvider::osm_light(), TileProvider::osm_dark()] {
            // те же плитки и тот же ключ кэша, отличается только фильтр вывода
            assert_eq!(p.id, osm.id);
            assert_eq!(p.tile_url(3, 5, 4), osm.tile_url(3, 5, 4));
            assert!(!p.filter.is_empty());
        }
        assert!(osm.filter.is_empty());
        assert_eq!(osm.tile_url(1, 2, 3), "https://tile.openstreetmap.org/3/1/2.png");
        #[allow(deprecated)]
        for p in [TileProvider::carto_light(), TileProvider::carto_dark(), TileProvider::carto_voyager()] {
            assert!(!p.tile_url(0, 0, 0).contains("cartocdn"));
        }
    }
}
