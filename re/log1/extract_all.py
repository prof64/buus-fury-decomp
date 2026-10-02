"""Regenerate every data/log1/*.json from the ROM (maps first: enemies.json uses maps.json)."""
import extract_maps, extract_enemies, extract_levels, extract_moves, extract_items, extract_characters, extract_formulas

for mod in (extract_maps, extract_enemies, extract_levels, extract_moves, extract_items, extract_characters,
            extract_formulas):
    mod.main()
