"""Portfolio weight optimization: the /portfolio/optimize endpoint and the
`weight_policy` field on /portfolio."""

import pytest
from fastapi.testclient import TestClient

from api.main import app

client = TestClient(app, raise_server_exceptions=False)

OBJECTIVES = [
    "equal_weight",
    "inverse_volatility",
    "min_variance",
    "risk_parity",
    "max_sharpe",
]


def available_datasets() -> list[str]:
    return [d["name"] for d in client.get("/datasets").json()["datasets"]]


@pytest.fixture(scope="module")
def datasets() -> list[str]:
    names = available_datasets()
    if len(names) < 2:
        pytest.skip("needs at least two datasets in the catalog")
    return names[:3]


def portfolio_body(names: list[str], **extra) -> dict:
    return {
        "assets": [
            {
                "symbol": n.rsplit(".", 1)[0],
                "weight": 1.0,
                "source": {"kind": "dataset", "name": n},
                "strategy": {"type": "rsi", "period": 14},
            }
            for n in names
        ],
        "initial_cash": 100_000.0,
        "commission": 0.0,
        "slippage_pct": 0.0,
        **extra,
    }


# ─── /portfolio/optimize ─────────────────────────────────────────────────────


@pytest.mark.parametrize("objective", OBJECTIVES)
def test_every_objective_returns_a_valid_long_only_portfolio(datasets, objective):
    r = client.post(
        "/portfolio/optimize",
        json={"datasets": datasets, "optimizer": {"objective": objective}},
    )
    assert r.status_code == 200, r.text
    body = r.json()
    weights = body["weights"]
    assert len(weights) == len(datasets)
    assert abs(sum(weights) - 1.0) < 1e-6, weights
    assert all(w >= -1e-12 for w in weights), weights


def test_response_names_the_symbols_it_solved_for(datasets):
    r = client.post("/portfolio/optimize", json={"datasets": datasets})
    body = r.json()
    assert body["symbols"] == [d.rsplit(".", 1)[0] for d in datasets]
    assert body["observations"] > 0


def test_risk_contributions_are_equal_under_risk_parity(datasets):
    r = client.post(
        "/portfolio/optimize",
        json={"datasets": datasets, "optimizer": {"objective": "risk_parity"}},
    )
    contributions = r.json()["risk_contribution"]
    target = 1.0 / len(contributions)
    assert all(abs(c - target) < 0.02 for c in contributions), contributions


def test_min_variance_is_not_riskier_than_equal_weight(datasets):
    """The defining property of the objective, checked end-to-end."""
    def vol(objective: str) -> float:
        return client.post(
            "/portfolio/optimize",
            json={"datasets": datasets, "optimizer": {"objective": objective, "shrinkage": 0.0}},
        ).json()["expected_volatility"]

    assert vol("min_variance") <= vol("equal_weight") + 1e-9


def test_position_cap_is_enforced(datasets):
    r = client.post(
        "/portfolio/optimize",
        json={
            "datasets": datasets,
            "optimizer": {"objective": "min_variance", "max_weight": 0.5},
        },
    )
    assert all(w <= 0.5 + 1e-6 for w in r.json()["weights"]), r.json()["weights"]


def test_only_max_sharpe_flags_a_dependence_on_expected_returns(datasets):
    """Mean returns are much noisier than covariance; the response says so."""
    for objective in OBJECTIVES:
        body = client.post(
            "/portfolio/optimize",
            json={"datasets": datasets, "optimizer": {"objective": objective}},
        ).json()
        assert body["uses_expected_returns"] == (objective == "max_sharpe"), objective


def test_lookback_limits_the_estimation_window(datasets):
    body = client.post(
        "/portfolio/optimize", json={"datasets": datasets, "lookback": 60}
    ).json()
    assert body["observations"] == 60


def test_null_lookback_uses_all_history(datasets):
    limited = client.post(
        "/portfolio/optimize", json={"datasets": datasets, "lookback": 60}
    ).json()["observations"]
    everything = client.post(
        "/portfolio/optimize", json={"datasets": datasets, "lookback": None}
    ).json()["observations"]
    assert everything > limited


def test_unknown_dataset_is_a_404(datasets):
    r = client.post("/portfolio/optimize", json={"datasets": [datasets[0], "NOPE.csv"]})
    assert r.status_code == 404


def test_a_single_dataset_is_rejected(datasets):
    r = client.post("/portfolio/optimize", json={"datasets": datasets[:1]})
    assert r.status_code == 422


@pytest.mark.parametrize(
    "optimizer",
    [
        {"objective": "not_a_thing"},
        {"shrinkage": 1.5},
        {"shrinkage": -0.1},
        {"max_weight": 0.0},
        {"max_weight": 1.5},
    ],
)
def test_invalid_optimizer_config_is_rejected(datasets, optimizer):
    r = client.post("/portfolio/optimize", json={"datasets": datasets, "optimizer": optimizer})
    assert r.status_code == 422, r.text


# ─── weight_policy on /portfolio ─────────────────────────────────────────────


def test_manual_policy_records_no_weight_history(datasets):
    r = client.post("/portfolio", json=portfolio_body(datasets))
    assert r.status_code == 200, r.text
    assert r.json().get("weight_history", []) == []


def test_static_policy_solves_once(datasets):
    r = client.post(
        "/portfolio",
        json=portfolio_body(
            datasets,
            weight_policy={
                "kind": "static",
                "warmup": 252,
                "optimizer": {"objective": "min_variance"},
            },
        ),
    )
    assert r.status_code == 200, r.text
    history = r.json()["weight_history"]
    assert len(history) == 1
    snap = history[0]
    assert abs(sum(snap["weights"]) - 1.0) < 1e-6
    assert "expected_volatility" in snap
    assert "risk_contribution" in snap


def test_dynamic_policy_resolves_repeatedly(datasets):
    r = client.post(
        "/portfolio",
        json=portfolio_body(
            datasets,
            rebalance={"frequency": {"kind": "monthly"}},
            weight_policy={
                "kind": "dynamic",
                "lookback": 252,
                "optimizer": {"objective": "risk_parity"},
            },
        ),
    )
    assert r.status_code == 200, r.text
    history = r.json()["weight_history"]
    assert len(history) > 5, f"only {len(history)} solves"
    dates = [s["date"] for s in history]
    assert dates == sorted(dates), "weight history must be chronological"


def test_dynamic_weights_actually_change_over_time(datasets):
    r = client.post(
        "/portfolio",
        json=portfolio_body(
            datasets,
            rebalance={"frequency": {"kind": "monthly"}},
            weight_policy={
                "kind": "dynamic",
                "lookback": 126,
                "optimizer": {"objective": "min_variance", "shrinkage": 0.0},
            },
        ),
    )
    history = r.json()["weight_history"]
    first, last = history[0]["weights"], history[-1]["weights"]
    assert first != last, "a dynamic policy that never moves is a static one"


def test_every_dynamic_snapshot_is_a_valid_portfolio(datasets):
    r = client.post(
        "/portfolio",
        json=portfolio_body(
            datasets,
            rebalance={"frequency": {"kind": "monthly"}},
            weight_policy={"kind": "dynamic", "lookback": 252},
        ),
    )
    for snap in r.json()["weight_history"]:
        assert abs(sum(snap["weights"]) - 1.0) < 1e-6, snap
        assert all(w >= -1e-12 for w in snap["weights"]), snap


def test_solved_runs_produce_a_finite_equity_curve(datasets):
    """Regression: a weight driven to zero once poisoned the curve with NaN."""
    r = client.post(
        "/portfolio",
        json=portfolio_body(
            datasets,
            rebalance={"frequency": {"kind": "monthly"}},
            weight_policy={
                "kind": "dynamic",
                "lookback": 126,
                "optimizer": {"objective": "min_variance", "shrinkage": 0.0},
            },
        ),
    )
    body = r.json()
    navs = [p["nav"] for p in body["equity_curve"]]
    assert all(n == n and n not in (float("inf"), float("-inf")) for n in navs)
    for key in ("total_return", "annualized_volatility", "sharpe_ratio"):
        value = body["metrics"][key]
        assert value == value, f"{key} was NaN"


def test_min_variance_beats_equal_weight_at_every_solve(datasets):
    """The defining property, checked through the whole stack.

    Both policies solve on the same trailing window at the same rebalance dates,
    so min-variance's predicted volatility must be no higher at any of them.

    Note what this does *not* claim: that *realized* volatility comes out lower.
    Only the in-sample optimum is guaranteed; whether it survives into the next
    month is an empirical question, and across closely-correlated large-caps the
    answer is often "barely". Principle 5 applies to weight solving exactly as it
    does to strategy parameters.
    """
    def solve(objective: str) -> dict[str, float]:
        body = client.post(
            "/portfolio",
            json=portfolio_body(
                datasets,
                rebalance={"frequency": {"kind": "monthly"}},
                weight_policy={
                    "kind": "dynamic",
                    "lookback": 252,
                    "optimizer": {"objective": objective, "shrinkage": 0.0},
                },
            ),
        ).json()
        return {s["date"]: s["expected_volatility"] for s in body["weight_history"]}

    minvar = solve("min_variance")
    equal = solve("equal_weight")
    assert minvar and minvar.keys() == equal.keys()
    for date, vol in minvar.items():
        assert vol <= equal[date] + 1e-9, (
            f"on {date} min-variance predicted {vol:.6f} vs equal weight {equal[date]:.6f}"
        )


def test_min_variance_is_not_merely_inverse_volatility(datasets):
    """Min-variance uses the correlation structure, so it can legitimately
    overweight a volatile asset that diversifies the rest. If the two objectives
    agreed everywhere, the covariance off-diagonals would be going unused."""
    def weights(objective: str) -> list[float]:
        return client.post(
            "/portfolio/optimize",
            json={
                "datasets": datasets,
                "optimizer": {"objective": objective, "shrinkage": 0.0},
            },
        ).json()["weights"]

    assert weights("min_variance") != weights("inverse_volatility")


def test_unknown_policy_kind_is_rejected(datasets):
    r = client.post(
        "/portfolio", json=portfolio_body(datasets, weight_policy={"kind": "psychic"})
    )
    assert r.status_code == 422


def test_warmup_below_two_is_rejected(datasets):
    r = client.post(
        "/portfolio",
        json=portfolio_body(datasets, weight_policy={"kind": "static", "warmup": 1}),
    )
    assert r.status_code == 422


def test_policy_round_trips_through_run_history(datasets):
    """The stored config must record how the weights were chosen."""
    policy = {
        "kind": "dynamic",
        "lookback": 252,
        "optimizer": {"objective": "risk_parity"},
    }
    run_id = client.post(
        "/portfolio", json=portfolio_body(datasets, weight_policy=policy)
    ).json()["run_id"]

    stored = client.get(f"/runs/{run_id}").json()
    assert stored["config"]["weight_policy"]["kind"] == "dynamic"
    assert stored["config"]["weight_policy"]["optimizer"]["objective"] == "risk_parity"
