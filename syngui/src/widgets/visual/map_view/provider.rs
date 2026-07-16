#[derive(Clone, Debug)]
pub struct TileProvider {
    pub id: u8,
    pub name: &'static str,
    url_template: &'static str,
    subdomains: &'static [&'static str],
    pub max_zoom: u8,
    pub attribution: &'static str,
}

impl TileProvider {
    pub fn osm() -> Self {
        Self {
            id: 0,
            name: "OpenStreetMap",
            url_template: "https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png",
            subdomains: &["a", "b", "c"],
            max_zoom: 19,
            attribution: "\u{00a9} OpenStreetMap contributors",
        }
    }

    pub fn carto_light() -> Self {
        Self {
            id: 1,
            name: "CartoDB Light",
            url_template: "https://{s}.basemaps.cartocdn.com/light_all/{z}/{x}/{y}.png",
            subdomains: &["a", "b", "c", "d"],
            max_zoom: 19,
            attribution: "\u{00a9} OpenStreetMap \u{00a9} CARTO",
        }
    }

    pub fn carto_dark() -> Self {
        Self {
            id: 2,
            name: "CartoDB Dark",
            url_template: "https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png",
            subdomains: &["a", "b", "c", "d"],
            max_zoom: 19,
            attribution: "\u{00a9} OpenStreetMap \u{00a9} CARTO",
        }
    }

    pub fn carto_voyager() -> Self {
        Self {
            id: 3,
            name: "CartoDB Voyager",
            url_template: "https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}.png",
            subdomains: &["a", "b", "c", "d"],
            max_zoom: 19,
            attribution: "\u{00a9} OpenStreetMap \u{00a9} CARTO",
        }
    }

    pub fn esri_satellite() -> Self {
        Self {
            id: 4,
            name: "ESRI Satellite",
            url_template: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
            subdomains: &[""],
            max_zoom: 18,
            attribution: "\u{00a9} Esri, DigitalGlobe, GeoEye",
        }
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
