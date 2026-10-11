import json
import math

import pandas as pd
import yfinance as yf


def _to_native(value):
    # Safely convert a DataFrame cell to a native python number (or None when NaN)
    if value is None or pd.isna(value):
        return None
    try:
        num = float(value)
    except (TypeError, ValueError):
        return None
    if math.isnan(num) or math.isinf(num):
        return None
    return int(num) if num.is_integer() else num


def _safe_lookup(df, key, date_col):
    # Return the cell value for a given row key and column date (or None if missing)
    try:
        return _to_native(df.loc[key, date_col])
    except (KeyError, TypeError):
        return None


def _latest_value(df, key):
    # Scan columns most-recent-first for the first non-NaN value of a row key
    if df is None or df.empty or key not in df.index:
        return None
    for col in df.columns:
        value = _to_native(df.loc[key, col])
        if value is not None:
            return value
    return None


def get_country(ticker):
    # Lightweight country lookup; returns None when unavailable
    try:
        return ticker.info.get("country")
    except Exception:
        try:
            return ticker.fast_info.get("country")
        except Exception:
            return None


def get_metadata(ticker):
    # Fetch lightweight metadata from fast_info, supplement beta/date from info
    meta = {}
    try:
        fi = ticker.fast_info
        meta["price"] = fi.last_price
        meta["market_cap"] = fi.market_cap
        meta["shares_outstanding"] = fi.shares
        meta["currency"] = fi.currency
        meta["exchange"] = fi.exchange
    except Exception:
        pass

    # beta + timestamp require the full info call
    try:
        info = ticker.info
        meta["beta"] = info.get("beta")
        meta["industry"] = info.get("industry")
        meta["sector"] = info.get("sector")
        ts = info.get("regularMarketTime")
        if ts:
            from datetime import datetime, timezone
            meta["date"] = datetime.fromtimestamp(ts, tz=timezone.utc).date().isoformat()
    except Exception:
        pass

    return meta


def get_price_only(ticker_symbol):
    # Fast price-only fetch — no income_stmt/balance_sheet DataFrames
    t = yf.Ticker(ticker_symbol)
    price = None
    currency = None
    price_date = None
    try:
        fi = t.fast_info
        price = _to_native(getattr(fi, "last_price", None))
        currency = _to_native(getattr(fi, "currency", None)) or None
        # last_trade_time may be datetime or None
        ltt = getattr(fi, "last_trade_time", None)
        if ltt is not None:
            try:
                # pandas Timestamp or datetime
                price_date = str(ltt.date()) if hasattr(ltt, "date") else str(ltt)
            except Exception:
                price_date = None
    except Exception:
        pass
    if price is None:
        # fallback: 1d history close
        try:
            h = t.history(period="1d")
            if h is not None and not h.empty:
                price = _to_native(h["Close"].iloc[-1])
                if price_date is None and h.index is not None and len(h.index) > 0:
                    price_date = str(h.index[-1].date())
        except Exception:
            pass
    # final fallback: info regularMarketPrice
    if price is None:
        try:
            info = t.info
            price = _to_native(info.get("regularMarketPrice") or info.get("currentPrice"))
            if currency is None:
                currency = info.get("currency")
            if price_date is None:
                ts = info.get("regularMarketTime")
                if ts:
                    from datetime import datetime, timezone
                    price_date = datetime.fromtimestamp(int(ts), tz=timezone.utc).date().isoformat()
        except Exception:
            pass
    return json.dumps({
        "symbol": ticker_symbol.upper(),
        "price": price,
        "currency": currency,
        "date": price_date,
    }, indent=4)


def get_5yo_financial_data(ticker_symbol):
    # Fetch ticker data via Yahoo Finance
    ticker = yf.Ticker(ticker_symbol)

    # 1. Fetch financial dataframes
    # Yahoo generally returns 4 standard years + 1 trailing or partial year depending on recent filings
    inc_stmt = ticker.income_stmt
    bal_sheet = ticker.balance_sheet
    cash_flow = ticker.cashflow
    ttm_income = ticker.ttm_income_stmt
    ttm_cash_flow = ticker.ttm_cashflow
    quarterly_bal_sheet = ticker.quarterly_balance_sheet

    # Map the exact keys used by yfinance
    # Note: Keys are case-sensitive and match the raw table indices
    metrics_mapping = {
        "revenue": (inc_stmt, "Total Revenue"),
        "operating_income": (inc_stmt, "Operating Income"),
        "ebitda": (inc_stmt, "EBITDA"),
        "net_income": (inc_stmt, "Net Income"),
        "total_debt": (bal_sheet, "Total Debt"),
        "total_equity": (bal_sheet, "Stockholders Equity"),
        "cash_equivalents": (bal_sheet, "Cash And Cash Equivalents"),
        "free_cash_flow": (cash_flow, "Free Cash Flow"),
    }

    output_data = {
        "symbol": ticker_symbol.upper(),
        "country": get_country(ticker),
    }
    output_data.update(get_metadata(ticker))

    # 2. TTM (trailing twelve months) snapshot
    ttm_row = {
        "revenue": _latest_value(ttm_income, "Total Revenue"),
        "operating_income": _latest_value(ttm_income, "Operating Income"),
        "ebitda": _latest_value(ttm_income, "EBITDA"),
        "net_income": _latest_value(ttm_income, "Net Income"),
        "free_cash_flow": _latest_value(ttm_cash_flow, "Free Cash Flow"),
    }
    # Balance-sheet items are point-in-time: prefer the most recent quarter,
    # otherwise fall back to the latest annual report
    for user_key, yf_key in {
        "total_debt": "Total Debt",
        "total_equity": "Stockholders Equity",
        "cash_equivalents": "Cash And Cash Equivalents",
    }.items():
        ttm_row[user_key] = _latest_value(quarterly_bal_sheet, yf_key) or _latest_value(
            bal_sheet, yf_key
        )
    output_data["ttm"] = ttm_row

    # 3. Last 4 fiscal years so TTM + years = 5 periods total
    #    (columns come most-recent first; keep whatever is available)
    years = list(inc_stmt.columns)[:4]
    for date_col in years:
        year_str = str(date_col.date())
        output_data[year_str] = {}

        for user_key, (df, yf_key) in metrics_mapping.items():
            output_data[year_str][user_key] = _safe_lookup(df, yf_key, date_col)

    return json.dumps(output_data, indent=4)


# --- Example Execution ---
# Replace 'MSFT' with any target stock ticker (e.g., 'AAPL', 'NVDA')
if __name__ == "__main__":
    #json_output = get_5yo_financial_data("RELIANCE.NS")
    #json_output = get_5yo_financial_data("NPST.NS")
    #json_output = get_5yo_financial_data("CIEN")
    json_output = get_5yo_financial_data("AAPL")
    print(json_output)
