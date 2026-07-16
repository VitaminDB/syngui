use syngui::prelude::*;
use syngui::widgets::*;
use super::{section_card, section_title, label};
use std::sync::{Arc, OnceLock};
use syngui::core::sync::Mutex;

/// Shared provider state — polled by MapViewElement::animate()
static PROVIDER_SOURCE: OnceLock<Arc<Mutex<TileProvider>>> = OnceLock::new();

fn get_provider(value: &str) -> TileProvider {
    match value {
        "carto_light" => TileProvider::carto_light(),
        "carto_dark" => TileProvider::carto_dark(),
        "carto_voyager" => TileProvider::carto_voyager(),
        "esri_satellite" => TileProvider::esri_satellite(),
        _ => TileProvider::osm(),
    }
}

pub fn build_map_section() -> impl Widget {
    let provider_source = PROVIDER_SOURCE.get_or_init(|| Arc::new(Mutex::new(TileProvider::osm())));

    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Map"))
            .child(label("Interactive map widget with pan, zoom, and markers"))
            .child(
                Row::new()
                    .gap(12.0)
                    .child(Text::new("Drag to pan, scroll to zoom").class("label"))
                    .child(
                        Dropdown::with_items(vec![
                            DropdownItem::new("osm", "OpenStreetMap"),
                            DropdownItem::new("carto_light", "CartoDB Light"),
                            DropdownItem::new("carto_dark", "CartoDB Dark"),
                            DropdownItem::new("carto_voyager", "CartoDB Voyager"),
                            DropdownItem::new("esri_satellite", "ESRI Satellite"),
                        ])
                        .selected("osm")
                        .placeholder("Map Style")
                        .width(220.0)
                        .on_change({
                            let source = provider_source.clone();
                            move |value| {
                                *source.lock().unwrap() = get_provider(value);
                            }
                        })
                    )
            )
            .child(
                MapView::new()
                    .center(53.2144, 63.6246) // Kostanay, Kazakhstan
                    .zoom(5)
                    .provider(TileProvider::osm())
                    .provider_source(provider_source.clone())
                    .height(1000.0)
                    .markers(vec![
                        MapMarker::new(53.2144, 63.6246)
                            .label("Kostanay")
                            .color(Color::from_hex("#E53935"))
                            .size(16.0),
                        MapMarker::new(51.1694, 71.4491)
                            .label("Astana")
                            .color(Color::from_hex("#1E88E5"))
                            .size(14.0),
                        MapMarker::new(43.2220, 76.8512)
                            .label("Almaty")
                            .color(Color::from_hex("#43A047"))
                            .size(14.0),
                        MapMarker::new(50.2839, 57.2350)
                            .label("Aktobe")
                            .color(Color::from_hex("#FB8C00"))
                            .size(12.0),
                        MapMarker::new(54.8753, 69.1351)
                            .label("Petropavlovsk")
                            .color(Color::from_hex("#8E24AA"))
                            .size(12.0),
                    ])
            )
    )
}
