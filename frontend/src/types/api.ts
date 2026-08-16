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

// ─── fill_timing ────────────────────────────────────────────────────────────
export type FillTiming = "close" | "next_open";

// ─── /backtest request ──────────────────────────────────────────────────────
export interface BacktestRequest {
  strategy: StrategyRequest;
  candles: Candle[];
  initial_cash: number;
  commission: number;
  slippage_pct: number;
  fill_timing?: FillTiming;
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

/** A two-sided percentile interval plus the bootstrap standard error. */
export interface Interval {
  lo: number;
  hi: number;
  std_error: number;
}

/**
 * Bootstrap uncertainty on the metrics that are functions of the return path.
 *
 * Absent unless the request asked for it, and absent on the sweep grid by design.
 * Note what it does *not* cover: selecting parameters by arg-max over a grid, or
 * repeated trials across assets and folds.
 */
export interface MetricUncertainty {
  method: string;
  confidence: number;
  resamples: number;
  mean_block: number;
  seed: number;
  observations: number;
  sharpe_ratio: Interval;
  sortino_ratio: Interval;
  cagr: Interval;
  /**
   * Spread only, no endpoints. Block resampling destroys the multi-month trends
   * that produce deep drawdowns, so percentile endpoints here would be biased
   * toward optimism and mislead about direction.
   */
  max_drawdown_std_error: number;
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
  uncertainty?: MetricUncertainty | null;
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

export type RebalanceFrequency =
  | { kind: "monthly" }
  | { kind: "quarterly" }
  | { kind: "threshold"; threshold: number };

export interface RebalanceConfig {
  frequency: RebalanceFrequency;
}

// ─── Weight optimization ────────────────────────────────────────────────────
// Mirrors backtesting/src/optimize.rs and the WeightPolicy in
// portfolio_backtest.rs.

export type Objective =
  | "equal_weight"
  | "inverse_volatility"
  | "min_variance"
  | "risk_parity"
  | "max_sharpe";

/** Objectives needing mean returns, which estimate far less reliably than covariance. */
export const RETURN_DEPENDENT_OBJECTIVES: ReadonlySet<Objective> = new Set<Objective>([
  "max_sharpe",
]);

export interface OptimizerConfig {
  objective: Objective;
  /** Covariance shrinkage toward the diagonal, 0–1. */
  shrinkage: number;
  /** Per-position cap in (0, 1]; null = uncapped. */
  max_weight: number | null;
}

export type WeightPolicy =
  | { kind: "manual" }
  | { kind: "static"; optimizer: OptimizerConfig; warmup: number }
  | { kind: "dynamic"; optimizer: OptimizerConfig; lookback: number };

export type WeightPolicyKind = WeightPolicy["kind"];

/** One change of target weights during a run. */
export interface WeightSnapshot {
  date: string;
  weights: number[];
  expected_volatility?: number;
  risk_contribution?: number[];
}

export interface OptimizeRequest {
  datasets: string[];
  optimizer: OptimizerConfig;
  /** Trailing bars to estimate from; null uses all history. */
  lookback: number | null;
}

export interface OptimizeResponse {
  symbols: string[];
  observations: number;
  /** The window actually estimated over: the intersection of the datasets'
   *  dates, which is generally narrower than any one dataset's full range. */
  window: { start: string; end: string };
  weights: number[];
  expected_volatility: number;
  expected_return: number;
  risk_contribution: number[];
  iterations: number;
  hit_iteration_limit: boolean;
  uses_expected_returns: boolean;
}

export interface PortfolioRequest {
  assets: PortfolioAssetRequest[];
  initial_cash: number;
  commission: number;
  slippage_pct: number;
  benchmark_symbol?: string;
  rebalance?: RebalanceConfig;
  /** Omit (or "manual") to use the weight on each asset. */
  weight_policy?: WeightPolicy;
  fill_timing?: FillTiming;
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
  external_benchmark_curve?: EquityPoint[];
  rebalance_dates?: string[];
  /** Present when a solving weight policy was used; one entry per solve. */
  weight_history?: WeightSnapshot[];
  run_id?: number;
}

// ─── /sweep ─────────────────────────────────────────────────────────────────
export interface ParamRange {
  min: number;
  max: number;
  step: number;
}

export interface SweepRequest {
  dataset: string;
  strategy_type: string;
  param_ranges: Record<string, ParamRange>;
  initial_cash: number;
  commission: number;
  slippage_pct: number;
  fill_timing?: FillTiming;
}

export interface SweepPoint {
  params: Record<string, number>;
  metrics: Metrics;
}

export interface SweepResponse {
  results: SweepPoint[];
}

// ─── /runs ──────────────────────────────────────────────────────────────────
// The scheduler writes its cycle summaries into the same `runs` table
// (api/scheduler.py), so this list interleaves backtests with tick summaries.
export type RunKind = "backtest" | "portfolio" | "scheduled_tick";

export interface RunListItem {
  id: number;
  created_at: string;
  kind: RunKind;
  config: BacktestRequest | PortfolioRequest | Record<string, unknown>;
}

export interface RunDetail extends RunListItem {
  result: BacktestResponse | PortfolioResponse | ScheduleCycle;
}

export interface RunsResponse {
  runs: RunListItem[];
}

// ─── /walkforward ───────────────────────────────────────────────────────────
export interface WalkForwardRequest {
  dataset: string;
  strategy_type: string;
  param_ranges: Record<string, ParamRange>;
  n_windows?: number;
  train_frac?: number;
  metric?: string;
  initial_cash?: number;
  commission?: number;
  slippage_pct?: number;
  fill_timing?: FillTiming;
}

export interface DateRange {
  start: string;
  end: string;
}

export interface WalkForwardFold {
  window_index: number;
  train_range: DateRange;
  test_range: DateRange;
  best_params: Record<string, number>;
  train_metrics: Metrics;
  test_metrics: Metrics;
  /** How many grid combos matched the winning train score. 1 = a clean win;
   *  higher means the metric could not discriminate and `best_params` was not
   *  really chosen; 0 means nothing was selectable. */
  tied_candidates: number;
}

export interface WalkForwardResponse {
  folds: WalkForwardFold[];
  /**
   * Metrics for every fold's out-of-sample returns stitched into one series.
   *
   * The honest headline for a walk-forward run, and not the average of the
   * per-fold numbers: averaging treats each fold as an independent observation,
   * pooling treats the sequence as the single dependent return path it is.
   */
  oos_metrics?: Metrics | null;
}

// ─── /trade/* — the live trading loop ───────────────────────────────────────
// Mirrors api/trading.py, api/scheduler.py, api/risk.py and api/broker.py.

export interface TradePlanItem {
  dataset: string;
  strategy: StrategyRequest;
  cash_allocation: number;
}

export interface TradePlan {
  plan_id: string;
  created_at: string;
  updated_at: string;
  enabled: boolean;
  items: TradePlanItem[];
}

export interface TradePlansResponse {
  plans: TradePlan[];
}

export interface TradePlanRequest {
  plan_id: string;
  items: TradePlanItem[];
  enabled: boolean;
}

/** The three guardrail fields; null means "no limit for this field". */
export interface RiskLimits {
  max_position_value: number | null;
  max_daily_loss: number | null;
  max_daily_orders: number | null;
}

export interface RiskLimitsRow extends RiskLimits {
  plan_id: string;
  updated_at: string;
}

export interface RiskLimitsRequest extends Partial<RiskLimits> {
  plan_id: string;
}

export interface LimitsListResponse {
  limits: RiskLimitsRow[];
}

/**
 * Limits for one plan. `effective` is what the loop actually enforces: the
 * plan's own row layered field-by-field over the global row.
 */
export interface LimitsForPlan {
  plan_id: string;
  effective: RiskLimits;
  plan: RiskLimitsRow | null;
  global: RiskLimitsRow | null;
}

export interface KillSwitch {
  engaged: boolean;
  reason: string | null;
  updated_at: string | null;
}

/** Order status as reported by the venue. */
export type OrderStatus =
  | "accepted"
  | "partially_filled"
  | "filled"
  | "rejected"
  | "canceled";

export interface OrderRow {
  id: number;
  created_at: string;
  updated_at: string;
  plan_id: string;
  symbol: string;
  strategy: string;
  side: "buy" | "sell";
  qty: number;
  broker: string;
  broker_order_id: string;
  status: OrderStatus;
  filled_qty: number;
  avg_fill_price: number;
  reason: string | null;
  intent_id: number | null;
}

export interface OrdersResponse {
  orders: OrderRow[];
}

export interface IntentRow {
  id: number;
  created_at: string;
  plan_id: string;
  symbol: string;
  strategy: string;
  side: "buy" | "sell";
  qty: number;
  price: number;
  signal: string;
  reason: string | null;
  status: string;
  realized_pnl: number | null;
}

export interface IntentsResponse {
  intents: IntentRow[];
}

export interface PositionRow {
  plan_id: string;
  symbol: string;
  strategy: string;
  qty: number;
  avg_price: number;
  updated_at: string;
}

export interface PositionsResponse {
  positions: PositionRow[];
}

export interface BrokerOrder {
  broker_order_id: string;
  status: OrderStatus;
  filled_qty: number;
  avg_fill_price: number;
  reason: string | null;
  ts: string;
}

export interface TradeIntent {
  side: "buy" | "sell";
  qty: number;
  price: number;
  reason: string;
  status: string;
  allowed: boolean;
  rejected_reason: string | null;
  realized_pnl: number | null;
  order: BrokerOrder | null;
}

export interface TickItemResult {
  symbol: string;
  signal: "buy" | "sell" | "hold";
  date: string;
  close: number;
  /** Null when the signal produced no action (e.g. hold, or already flat). */
  intent: TradeIntent | null;
}

export interface DriftRow {
  symbol: string;
  ledger_qty: number;
  broker_qty: number;
  delta: number;
}

export interface ReconcileResult {
  broker: string;
  in_sync: boolean;
  checked: number;
  drift: DriftRow[];
}

export interface SyncedOrder {
  order_id: number;
  symbol: string;
  from: OrderStatus;
  to: OrderStatus;
  newly_filled: number;
}

export interface TickResult {
  plan_id: string;
  results: TickItemResult[];
  positions: PositionRow[];
  limits: RiskLimits;
  kill_switch: KillSwitch;
  synced_orders: SyncedOrder[];
  reconciliation: ReconcileResult;
}

export interface TradeTickRequest {
  plan_id: string;
  items?: TradePlanItem[];
}

export interface RefreshResult {
  ticker: string;
  status: "ok" | "no_data" | "error";
  added?: number;
  total?: number;
  start?: string;
  end?: string;
  error?: string;
}

/** One plan's outcome inside a scheduled cycle. */
export type CyclePlanResult =
  | (TickResult & { status: "ok" })
  | { plan_id: string; status: "error"; error: string };

export interface ScheduleCycle {
  started_at: string;
  finished_at: string;
  refreshed: RefreshResult[];
  plans_run: number;
  plans_failed: number;
  intents_emitted: number;
  results: CyclePlanResult[];
}

export interface CronConfig {
  day_of_week: string;
  hour: number;
  minute: number;
  timezone: string;
}

export interface ScheduleStatus {
  enabled: boolean;
  running: boolean;
  refresh_data: boolean;
  cron: CronConfig;
  next_run: string | null;
  /** The most recent `scheduled_tick` run row, result included. */
  last_run: (RunListItem & { result: ScheduleCycle }) | null;
}

export interface BrokerInfo {
  kind: string;
  slippage: number;
  fill_ratio: number;
  name: string;
  /** True when orders reach a real venue. Gates fail-closed risk checks. */
  is_live: boolean;
}

export interface SoakReport {
  plan_id: string | null;
  orders: number;
  filled: number;
  rejected: number;
  /** Filled qty over requested qty; null when nothing was requested. */
  fill_rate: number | null;
  slippage_bps: {
    samples: number;
    mean: number | null;
    worst: number | null;
    note: string;
  };
  realized_pnl: number;
}

// FastAPI error envelope (422 has a detail array; 503 has a detail string).
export interface ApiErrorBody {
  detail?:
    | string
    | Array<{ loc: (string | number)[]; msg: string; type: string }>;
}
