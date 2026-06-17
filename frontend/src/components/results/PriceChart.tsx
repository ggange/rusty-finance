import { useEffect, useRef } from "react";
import {
  createChart,
  ColorType,
  type CandlestickData,
  type IChartApi,
  type SeriesMarker,
  type Time,
} from "lightweight-charts";
import type { Candle, Trade } from "../../types/api";

interface PriceChartProps {
  candles: Candle[];
  trades: Trade[];
}

export function PriceChart({ candles, trades }: PriceChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const chart: IChartApi = createChart(container, {
      width: container.clientWidth,
      height: 320,
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: "#94a3b8",
      },
      grid: {
        vertLines: { color: "#1e293b" },
        horzLines: { color: "#1e293b" },
      },
      timeScale: { borderColor: "#334155" },
      rightPriceScale: { borderColor: "#334155" },
    });

    const series = chart.addCandlestickSeries({
      upColor: "#10b981",
      downColor: "#ef4444",
      borderUpColor: "#10b981",
      borderDownColor: "#ef4444",
      wickUpColor: "#10b981",
      wickDownColor: "#ef4444",
    });

    series.setData(
      candles.map(
        (c): CandlestickData => ({
          time: c.date as Time,
          open: c.open,
          high: c.high,
          low: c.low,
          close: c.close,
        }),
      ),
    );

    // Markers must be sorted ascending by time. Trade dates coincide with candle
    // dates, so each marker anchors cleanly to its bar.
    const markers: SeriesMarker<Time>[] = trades
      .slice()
      .sort((a, b) => a.date.localeCompare(b.date))
      .map((t) => ({
        time: t.date as Time,
        position: t.action === "Buy" ? "belowBar" : "aboveBar",
        color: t.action === "Buy" ? "#10b981" : "#ef4444",
        shape: t.action === "Buy" ? "arrowUp" : "arrowDown",
        text: `${t.action} ${t.shares}@${t.price.toFixed(2)}`,
      }));
    series.setMarkers(markers);

    chart.timeScale().fitContent();

    const ro = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width;
      if (width) chart.applyOptions({ width });
    });
    ro.observe(container);

    return () => {
      ro.disconnect();
      chart.remove();
    };
  }, [candles, trades]);

  return <div ref={containerRef} className="w-full" />;
}
