import argparse
import json
import sys
import time

from yfinance.const import EQUITY_SCREENER_EQ_MAP
from yfinance.screener import ETFQuery, EquityQuery, FundQuery, screen

PAGE_SIZE = 250  # Yahoo screener max results per request

QUERY_TYPES = {
    "equity": EquityQuery,
    "etf": ETFQuery,
    "fund": FundQuery,
}

# Friendly aliases -> Yahoo region codes
COUNTRY_ALIASES = {
    "usa": "us",
    "united states": "us",
    "uk": "gb",
    "united kingdom": "gb",
    "india": "in",
    "china": "cn",
    "japan": "jp",
    "germany": "de",
    "france": "fr",
    "canada": "ca",
    "australia": "au",
    "brazil": "br",
    "south korea": "kr",
    "korea": "kr",
    "hong kong": "hk",
    "singapore": "sg",
    "switzerland": "ch",
    "netherlands": "nl",
    "spain": "es",
    "italy": "it",
    "sweden": "se",
    "saudi arabia": "sa",
    "uae": "ae",
    "south africa": "za",
}


def valid_regions():
    return sorted(list(EQUITY_SCREENER_EQ_MAP.get("region", [])))


def resolve_region(country):
    code = country.strip().lower()
    code = COUNTRY_ALIASES.get(code, code)
    if code not in valid_regions():
        raise ValueError(
            f"Unknown country/region '{country}'. Valid codes: {', '.join(valid_regions())}"
        )
    return code


def _suffix_rank(symbol, prefer_suffixes):
    s = (symbol or "").upper()
    for i, suffix in enumerate(prefer_suffixes):
        if s.endswith(suffix.upper()):
            return i
    return len(prefer_suffixes)


def _pick_preferred(current, candidate, prefer_suffixes):
    # Keep the listing whose exchange suffix ranks higher (e.g. .NS over .BO)
    if _suffix_rank(candidate.get("symbol"), prefer_suffixes) < _suffix_rank(
        current.get("symbol"), prefer_suffixes
    ):
        return candidate
    return current


def _dedupe_key(ticker):
    # Prefer the base symbol (e.g. RELIANCE from RELIANCE.NS / RELIANCE.BO)
    # so dual listings collapse even when Yahoo omits the company name.
    symbol = ticker.get("symbol") or ""
    base = symbol.split(".")[0].strip().upper()
    if base:
        return base
    return (ticker.get("name") or "").strip().lower()


def dedupe_tickers(tickers, prefer_suffixes=(".NS", ".BO")):
    # Collapse dual listings of the same company into one entry,
    # preferring the exchange suffix order given (default: NSE .NS over BSE .BO)
    unique = {}
    for ticker in tickers:
        key = _dedupe_key(ticker)
        if key in unique:
            unique[key] = _pick_preferred(unique[key], ticker, prefer_suffixes)
        else:
            unique[key] = ticker
    return list(unique.values())


def get_tickers_for_country(
    country, quote_type="equity", limit=None, pause=0.4, dedupe=True
):
    region = resolve_region(country)
    query_cls = QUERY_TYPES[quote_type.lower()]
    query = query_cls("is-in", ["region", region])

    tickers = []
    offset = 0
    total = None

    while True:
        result = screen(query, size=PAGE_SIZE, offset=offset)
        quotes = result.get("quotes", [])
        if total is None:
            total = result.get("total", 0)

        for q in quotes:
            name = q.get("longName") or q.get("shortName")
            if not name:
                continue
            tickers.append(
                {
                    "symbol": q.get("symbol"),
                    "name": name,
                    "exchange": q.get("exchange"),
                    "currency": q.get("currency"),
                    "market": q.get("market"),
                }
            )
            if limit and len(tickers) >= limit:
                return (dedupe_tickers(tickers) if dedupe else tickers), total

        if not quotes or len(tickers) >= total:
            break

        offset += PAGE_SIZE
        time.sleep(pause)

    return (dedupe_tickers(tickers) if dedupe else tickers), total


def main():
    parser = argparse.ArgumentParser(
        description="List all Yahoo Finance tickers for a given country."
    )
    parser.add_argument(
        "country",
        help="Country name or Yahoo region code (e.g. india, in, usa, us, uk, gb)",
    )
    parser.add_argument(
        "--type",
        default="equity",
        choices=sorted(QUERY_TYPES.keys()),
        help="Instrument type to list (default: equity)",
    )
    parser.add_argument(
        "--limit", type=int, default=None, help="Stop after N tickers"
    )
    parser.add_argument(
        "--out", default=None, help="Output JSON file path (default: tickers_<region>.json)"
    )
    parser.add_argument(
        "--no-dedup",
        action="store_true",
        help="Keep dual listings (e.g. both .NS and .BO) instead of collapsing by name",
    )
    args = parser.parse_args()

    try:
        region = resolve_region(args.country)
    except ValueError as exc:
        print(exc, file=sys.stderr)
        sys.exit(1)

    tickers, total = get_tickers_for_country(
        args.country, quote_type=args.type, limit=args.limit, dedupe=not args.no_dedup
    )

    out_path = args.out or f"tickers_{region.upper()}.json"
    with open(out_path, "w") as fh:
        json.dump(
            {
                "region": region,
                "type": args.type,
                "reported_total": total,
                "count": len(tickers),
                "deduped": not args.no_dedup,
                "tickers": tickers,
            },
            fh,
            indent=4,
        )

    print(f"Country/region: {region} ({args.type})")
    print(f"Reported total: {total}")
    print(f"Fetched: {len(tickers)} tickers (deduped: {not args.no_dedup}) -> {out_path}")


if __name__ == "__main__":
    main()
