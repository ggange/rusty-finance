import type { Candle, Metrics, PortfolioResponse } from "../types/api";

export const metrics: Metrics = {
  total_return: 0.42,
  cagr: 0.18,
  annualized_volatility: 0.21,
  max_drawdown: -0.13,
  sharpe_ratio: 1.18,
  sortino_ratio: 1.64,
  win_rate: 0.57,
  trade_count: 12,
};

export const candles: Candle[] = [
  { date: "2026-01-02", open: 100, high: 104, low: 99, close: 102, volume: 1_000 },
  { date: "2026-01-03", open: 102, high: 108, low: 101, close: 107, volume: 1_200 },
  { date: "2026-01-04", open: 107, high: 110, low: 104, close: 105, volume: 900 },
];

export const portfolioResult: PortfolioResponse = {
  equity_curve: [
    { date: "2026-01-02", nav: 100_000 },
    { date: "2026-01-03", nav: 104_500 },
    { date: "2026-01-04", nav: 103_200 },
  ],
  metrics,
  benchmark: { total_return: 0.03, cagr: 0.11 },
  risk: {
    symbols: ["MSFT", "NVDA"],
    correlation: [
      [1, 0.62],
      [0.62, 1],
    ],
    covariance: [
      [0.04, 0.02],
      [0.02, 0.06],
    ],
    asset_volatility: [0.2, 0.31],
    asset_beta: [0.95, 1.42],
    contribution_to_risk: [0.38, 0.62],
    rolling_volatility: [
      { date: "2026-01-03", value: 0.19 },
      { date: "2026-01-04", value: 0.22 },
    ],
    var_95: -0.021,
    cvar_95: -0.03,
    var_99: -0.041,
    cvar_99: -0.052,
  },
  assets: [
    {
      symbol: "MSFT",
      weight: 0.5,
      allocated_cash: 50_000,
      equity_curve: [
        { date: "2026-01-02", nav: 50_000 },
        { date: "2026-01-03", nav: 52_000 },
        { date: "2026-01-04", nav: 51_600 },
      ],
      trades: [
        {
          date: "2026-01-02",
          action: "Buy",
          shares: 490,
          price: 102,
          commission: 1,
          cash_after: 20,
          pnl: null,
        },
        {
          date: "2026-01-04",
          action: "Sell",
          shares: 490,
          price: 105,
          commission: 1,
          cash_after: 51_450,
          pnl: 1_468,
        },
      ],
      metrics,
      benchmark: { total_return: 0.03, cagr: 0.11 },
    },
  ],
  rebalance_dates: ["2026-01-03"],
  run_id: 7,
};
