// The map's look (issue #212): the charter's warm paper, ink and one red, read from the
// interface's own colours so that the map is light or dark with it. Without the base
// map, the places sit on a plain ground and nothing is fetched; with it, OpenFreeMap's
// vector tiles (OpenStreetMap's data, the OpenMapTiles schema) are drawn in those
// colours. No other colour is added.
import type { ExpressionSpecification, LayerSpecification, StyleSpecification } from 'maplibre-gl';
import type { FeatureCollection } from 'geojson';
import type { MapPlaceView } from './api.ts';

/** Where the base map comes from: its tiles and its letters. */
export const TILES_HOST = 'https://tiles.openfreemap.org';
const TILES = `${TILES_HOST}/planet`;
const GLYPHS = `${TILES_HOST}/fonts/{fontstack}/{range}.pbf`;
const FONT = ['Noto Sans Regular'];

type Colours = { ground: string; surface: string; text: string; muted: string; line: string; lineStrong: string; thread: string; threadInk: string; faint: string };

/** The interface's colours as they are now (light or dark). */
export function colours(): Colours {
	const css = getComputedStyle(document.documentElement);
	const read = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
	return {
		ground: read('--ground', '#fbfaf8'),
		surface: read('--surface', '#ffffff'),
		text: read('--text', '#231d1e'),
		muted: read('--muted', '#6e6466'),
		line: read('--line', '#eee9e7'),
		lineStrong: read('--line-strong', '#e5dedc'),
		thread: read('--thread', '#9b2c3a'),
		threadInk: read('--thread-ink', '#94313f'),
		faint: read('--thread-faint', '#e8c9cd')
	};
}

/** The places, as points: their id, name and visits. */
export function placesData(places: MapPlaceView[]): FeatureCollection {
	return {
		type: 'FeatureCollection',
		features: places.map((p) => ({
			type: 'Feature',
			id: undefined,
			geometry: { type: 'Point', coordinates: [p.lon, p.lat] },
			properties: { id: p.id, name: p.name, visits: p.visits }
		}))
	};
}

const radius: ExpressionSpecification = ['+', 5, ['*', 2.6, ['sqrt', ['get', 'visits']]]];

/** The places' layers: groups of close places, then each place, then the chosen one. */
function placeLayers(c: Colours, selected: string | null): LayerSpecification[] {
	return [
		{
			id: 'groups',
			type: 'circle',
			source: 'places',
			filter: ['has', 'point_count'],
			paint: {
				'circle-color': c.thread,
				'circle-opacity': 0.35,
				'circle-stroke-color': c.threadInk,
				'circle-stroke-width': 1.5,
				'circle-radius': ['+', 10, ['*', 2.2, ['sqrt', ['get', 'visits']]]]
			}
		},
		{
			id: 'places',
			type: 'circle',
			source: 'places',
			filter: ['!', ['has', 'point_count']],
			paint: {
				'circle-color': c.thread,
				'circle-opacity': 0.55,
				'circle-stroke-color': c.threadInk,
				'circle-stroke-width': 1.5,
				'circle-radius': radius
			}
		},
		{
			id: 'chosen',
			type: 'circle',
			source: 'places',
			filter: ['all', ['!', ['has', 'point_count']], ['==', ['get', 'id'], selected ?? '']],
			paint: {
				'circle-color': c.thread,
				'circle-opacity': 0.9,
				'circle-stroke-color': c.text,
				'circle-stroke-width': 2.5,
				'circle-radius': radius
			}
		}
	];
}

/** The base map's layers, in the interface's colours. */
function baseLayers(c: Colours): LayerSpecification[] {
	const name: ExpressionSpecification = ['coalesce', ['get', 'name:latin'], ['get', 'name']];
	const label = { 'text-color': c.muted, 'text-halo-color': c.ground, 'text-halo-width': 1.4 };
	const roads = (classes: string[], width: number, minzoom = 0): LayerSpecification => ({
		id: `roads-${classes[0]}`,
		type: 'line',
		source: 'base',
		'source-layer': 'transportation',
		minzoom,
		filter: ['in', ['get', 'class'], ['literal', classes]],
		layout: { 'line-cap': 'round', 'line-join': 'round' },
		paint: {
			'line-color': c.lineStrong,
			'line-width': ['interpolate', ['exponential', 1.6], ['zoom'], 6, width * 0.3, 18, width * 8]
		}
	});
	return [
		{ id: 'parks', type: 'fill', source: 'base', 'source-layer': 'park', paint: { 'fill-color': c.line, 'fill-opacity': 0.7 } },
		{
			id: 'green',
			type: 'fill',
			source: 'base',
			'source-layer': 'landcover',
			filter: ['in', ['get', 'class'], ['literal', ['wood', 'grass']]],
			paint: { 'fill-color': c.line, 'fill-opacity': 0.5 }
		},
		{ id: 'water', type: 'fill', source: 'base', 'source-layer': 'water', paint: { 'fill-color': c.lineStrong } },
		{ id: 'rivers', type: 'line', source: 'base', 'source-layer': 'waterway', paint: { 'line-color': c.lineStrong, 'line-width': 1.2 } },
		{ id: 'buildings', type: 'fill', source: 'base', 'source-layer': 'building', minzoom: 14, paint: { 'fill-color': c.line } },
		roads(['minor', 'service', 'track', 'path'], 0.8, 12),
		roads(['tertiary', 'secondary'], 1.2, 9),
		roads(['primary', 'trunk'], 1.6, 6),
		roads(['motorway'], 2, 4),
		{
			id: 'borders',
			type: 'line',
			source: 'base',
			'source-layer': 'boundary',
			filter: ['all', ['<=', ['get', 'admin_level'], 2], ['!=', ['get', 'maritime'], 1]],
			paint: { 'line-color': c.muted, 'line-width': 1, 'line-dasharray': [3, 2], 'line-opacity': 0.6 }
		},
		{
			id: 'street-names',
			type: 'symbol',
			source: 'base',
			'source-layer': 'transportation_name',
			minzoom: 15,
			layout: { 'symbol-placement': 'line', 'text-field': name, 'text-font': FONT, 'text-size': 11 },
			paint: label
		},
		{
			id: 'place-names',
			type: 'symbol',
			source: 'base',
			'source-layer': 'place',
			filter: ['in', ['get', 'class'], ['literal', ['city', 'town', 'village', 'suburb', 'country']]],
			layout: {
				'text-field': name,
				'text-font': FONT,
				'text-size': ['match', ['get', 'class'], 'country', 13, 'city', 14, 'town', 12, 11],
				'symbol-sort-key': ['get', 'rank']
			},
			paint: { ...label, 'text-color': c.text }
		}
	];
}

/** The whole style: the ground, the base map when the person turned it on, the places. */
export function mapStyle(places: MapPlaceView[], selected: string | null, tiles: boolean): StyleSpecification {
	const c = colours();
	return {
		version: 8,
		...(tiles ? { glyphs: GLYPHS } : {}),
		sources: {
			...(tiles ? { base: { type: 'vector' as const, url: TILES } } : {}),
			places: {
				type: 'geojson',
				data: placesData(places),
				cluster: true,
				clusterRadius: 28,
				clusterMaxZoom: 15,
				clusterProperties: { visits: ['+', ['get', 'visits']] }
			}
		},
		layers: [{ id: 'ground', type: 'background', paint: { 'background-color': tiles ? c.ground : c.surface } }, ...(tiles ? baseLayers(c) : []), ...placeLayers(c, selected)]
	};
}
