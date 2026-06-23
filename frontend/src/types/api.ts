// TypeScript mirror of the FastAPI contract in api/main.py.

// ─── Candle (lowercase keys the API expects) ────────────────────────────────
export interface Candle {
  date: string; // "YYYY-MM-DD"
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

// ─── Strategy selection (request side) ──────────────────────────────────────
export type StrategyType = "ma_ema" | "ma_sma" | "ma_wma" | "rsi" | "macd" | "bollinger_bands";

export interface MAStrategy {
  type: "ma_ema" | "ma_sma" | "ma_wma";
  short_window: number;
  long_window: number;
}

export interface RSIStrategy {
  type: "rsi";
  period: number;
}

export interface MACDStrategy {
  type: "macd";
  fast_period: number;
  slow_period: number;
  signal_period: number;
}

export interface BollingerBandsStrategy {
  type: "bollinger_bands";
  period: number;
  std_dev_mult: number;
}

export type StrategyRequest = MAStrategy | RSIStrategy | MACDStrategy | BollingerBandsStrategy;

// ─── /strategies registry (metadata, response side) ─────────────────────────
export interface StrategyParamMeta {
  name: string;
  type: "integer" | "number";
  default: number;
  min: number;
  step?: number;
  description?: string;
}

export interface StrategyMeta {
  type: StrategyType;
  name: string; // human label, e.g. "EMA Crossover"
  description: string;
  params: StrategyParamMeta[];
}

export interface StrategiesResponse {
  strategies: StrategyMeta[];
}

// ─── /health ────────────────────────────────────────────────────────────────
export interface HealthResponse {
  status: "ok";
  engine: "available" | "unavailable";
}

// ─── /backtest request ──────────────────────────────────────────────────────
export interface BacktestRequest {
  strategy: StrategyRequest;
  candles: Candle[];
  initial_cash: number;
  commission: number;
  slippage_pct: number;
}

// ─── /backtest response ─────────────────────────────────────────────────────
export interface EquityPoint {
  date: string;
  nav: number;
}

export type TradeAction = "Buy" | "Sell";

export interface Trade {
  date: string;
  action: TradeAction;
  shares: number;
  price: number;
  commission: number;
  cash_after: number;
  pnl: number | null; // null on Buy
}

export interface Metrics {
  total_return: number;
  cagr: number;
  annualized_volatility: number;
  max_drawdown: number;
  sharpe_ratio: number;
  sortino_ratio: number;
  win_rate: number | null;
  trade_count: number;
}

export interface Benchmark {
  total_return: number;
  cagr: number;
}

export interface BacktestResponse {
  equity_curve: EquityPoint[];
  trades: Trade[];
  metrics: Metrics;
  benchmark: Benchmark;
  run_id?: number;
}

// ─── /datasets (server-side catalog) ────────────────────────────────────────
export interface Dataset {
  name: string; // file name, e.g. "AAPL.csv"
  symbol: string; // file stem, e.g. "AAPL"
  rows: number;
  start: string;
  end: string;
}

export interface DatasetsResponse {
  datasets: Dataset[];
}

export interface DatasetCandlesResponse {
  name: string;
  candles: Candle[];
}

// ─── /portfolio request ─────────────────────────────────────────────────────
export type AssetSource =
  | { kind: "dataset"; name: string }
  | { kind: "inline"; candles: Candle[] };

export interface PortfolioAssetRequest {
  symbol: string;
  weight: number;
  source: AssetSource;
  strategy: StrategyRequest;
}

export interface PortfolioRequest {
  assets: PortfolioAssetRequest[];
  initial_cash: number;
  commission: number;
  slippage_pct: number;
}

// ─── /portfolio response ────────────────────────────────────────────────────
// Each asset flattens a BacktestResponse plus its allocation metadata.
export interface AssetResult extends BacktestResponse {
  symbol: string;
  weight: number;
  allocated_cash: number;
}

export interface RollingPoint {
  date: string;
  value: number;
}

export interface RiskMetrics {
  symbols: string[];
  correlation: number[][];
  covariance: number[][];
  asset_volatility: number[];
  asset_beta: number[];
  contribution_to_risk: number[];
  rolling_volatility: RollingPoint[];
  var_95: number;
  cvar_95: number;
  var_99: number;
  cvar_99: number;
}

export interface PortfolioResponse {
  equity_curve: EquityPoint[];
  metrics: Metrics;
  benchmark: Benchmark;
  risk: RiskMetrics;
  assets: AssetResult[];
  run_id?: number;
}

// ─── /runs ──────────────────────────────────────────────────────────────────
export interface RunListItem {
  id: number;
  created_at: string;
  kind: "backtest" | "portfolio";
  config: BacktestRequest | PortfolioRequest;
}

export interface RunDetail extends RunListItem {
  result: BacktestResponse | PortfolioResponse;
}

export interface RunsResponse {
  runs: RunListItem[];
}

// FastAPI error envelope (422 has a detail array; 503 has a detail string).
export interface ApiErrorBody {
  detail?:
    | string
    | Array<{ loc: (string | number)[]; msg: string; type: string }>;
}
