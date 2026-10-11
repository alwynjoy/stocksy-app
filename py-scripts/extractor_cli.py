#!/usr/bin/env python3
import sys
import json
import argparse

import yahoo_stock_extract
import yfinance as yf


def extract_single(ticker: str) -> dict:
    raw_str = yahoo_stock_extract.get_5yo_financial_data(ticker)
    out = json.loads(raw_str)
    try:
        t = yf.Ticker(ticker)
        info = t.info if hasattr(t, "info") else {}
        out["company_name"] = info.get("longName") or info.get("shortName") or ticker
    except Exception:
        out["company_name"] = out.get("company_name") or ticker
    return out


def quote_single(ticker: str) -> str:
    return yahoo_stock_extract.get_price_only(ticker)


def batch_extract(tickers: list) -> dict:
    return {t: extract_single(t) for t in tickers}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Stocksy Yahoo Extractor CLI")
    subparsers = parser.add_subparsers(dest="command")

    p_ext = subparsers.add_parser("extract", help="Full 5y + TTM extraction")
    p_ext.add_argument("ticker", type=str)

    p_quo = subparsers.add_parser("quote", help="Fast price-only quote")
    p_quo.add_argument("ticker", type=str)

    p_bat = subparsers.add_parser("batch", help="Batch extract multiple tickers")
    p_bat.add_argument("tickers", nargs="+", type=str)

    args = parser.parse_args()

    try:
        if args.command == "extract":
            print(json.dumps(extract_single(args.ticker)))
        elif args.command == "quote":
            print(quote_single(args.ticker))
        elif args.command == "batch":
            print(json.dumps(batch_extract(args.tickers)))
        else:
            parser.print_help()
            sys.exit(1)
    except Exception as e:
        import traceback

        print(json.dumps({"_error": str(e), "trace": traceback.format_exc()}))
        sys.exit(1)
