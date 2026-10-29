import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("scraper", Path(__file__).with_name("scrape-foreverdb-prices.py"))
scraper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scraper)


class PriceExtractionTests(unittest.TestCase):
    def extract(self, record):
        return scraper.extract_item(1180, record, {1180: "Scroll of Stamina"}, "public-chart.json", "2026-10-29", "2026-10-25")

    def test_only_normal_horde_and_distinct_realms(self):
        record = {"hist": [["2026-10-13", 100, 200, 4]], "u": 2, "s": 5, "c": 1}
        rows, latest = self.extract({"h": {"Classic Beta PvE|H": record, "Classic Beta PvE 2|H": record,
                                          "Classic Beta PvP|H": record, "Classic Beta PvE|A": record,
                                          "Classic Beta PvE 2|N": record}})
        self.assertEqual(len(rows), 2)
        self.assertEqual({r["realm"] for r in rows}, set(scraper.REALMS))
        self.assertTrue(all(r["house"] == "Horde" for r in rows))
        self.assertTrue(all(r["daily_scan_count"] is None and r["daily_confirmation"] == "unknown" for r in rows))
        self.assertTrue(all(r["confirmed"] for r in latest))

    def test_missing_days_and_bid_only_prices_are_not_invented(self):
        rows, _ = self.extract({"h": {"Classic Beta PvE 2|H": {"hist": [["2026-10-03", 0, 0, 3], ["2026-10-20", 100, 200, 0]]}}})
        self.assertEqual(len(rows), 2)
        self.assertIsNone(rows[0]["min_buyout_copper"])
        self.assertIsNone(rows[0]["median_buyout_copper"])
        self.assertEqual(rows[1]["listed_quantity"], 0)

    def test_bad_shapes_duplicates_and_negative_prices_fail(self):
        for points in [ [["2026-10-03", 1, 2]], [["2026-10-03", -1, 2, 3]],
                        [["2026-10-03", 1, 2, 3], ["2026-10-03", 4, 5, 6]] ]:
            with self.assertRaises(ValueError):
                self.extract({"h": {"Classic Beta PvE|H": {"hist": points}}})

    def test_variants_are_marked_as_combined(self):
        rows, _ = self.extract({"v": 3, "h": {"Classic Beta PvE 2|H": {"hist": [["2026-10-03", 1, 2, 3]]}}})
        self.assertTrue(rows[0]["variants_combined"])
        self.assertEqual(rows[0]["variant_count"], 3)


if __name__ == "__main__":
    unittest.main()
