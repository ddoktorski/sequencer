empty_dashboard = {
    "annotations": {
        "list": [
            {
                "builtIn": 1,
                "datasource": {"type": "grafana", "uid": "-- Grafana --"},
                "enable": True,
                "hide": True,
                "iconColor": "rgba(0, 211, 255, 1)",
                "name": "Annotations & Alerts",
                "type": "dashboard",
            }
        ]
    },
    "editable": True,
    "fiscalYearStartMonth": 0,
    "graphTooltip": 0,
    "links": [],
    "liveNow": False,
    "panels": [],
    "refresh": "5s",
    "schemaVersion": 38,
    "style": "dark",
    "tags": [],
    "templating": {"list": []},
    "time": {"from": "now-6h", "to": "now"},
    "timepicker": {},
    "timezone": "",
    "title": "New dashboard",
    "version": 0,
    "weekStart": "",
}

row_object = {
    "collapsed": True,
    "gridPos": {"h": 1, "w": 24, "x": 0, "y": 0},
    "id": 1,
    "panels": [],
    "title": "Row title 1",
    "type": "row",
}

alert_query_model_object = {
    "editorMode": "code",
    "expr": "batcher_proposal_started{}",
    "instant": True,
    "intervalMs": 1000,
    "legendFormat": "__auto",
    "maxDataPoints": 43200,
    "range": False,
    "refId": "A",
}

alert_query_object = {
    "refId": "",
    "relativeTimeRange": {"from": 600, "to": 0},
    "datasourceUid": "",
    "queryType": "",
    "model": {},
}

alert_rule_object = {
    "name": "", # Required
    "title": "", # Required
    "orgId": 1,
    "condition": "",
    "interval": "1m",
    "uid": "",
    "data": [],
    "for": "5m",
    "execErrState": "Error",
    "noDataState": "NoData",
    "folderUID": "",
    "ruleGroup": "",
    "annotations": {},
    "labels": {},
    "isPaused": False,
}

alert_rule_object_tmp = {
    "orgId": 1,
    "name": "batcher", # Required
    "folderUID": "aeg7z7nx5ryf4b",
    "interval": "1m",
    "uid": "",
    "title": "testrule3",
    "condition": "B",
    "data": [
        {
            "refId": "A",
            "relativeTimeRange": {"from": 600, "to": 0},
            "datasourceUid": "PBFA97CFB590B2093",
            "model": {
                "editorMode": "code",
                "expr": "batcher_proposal_started{}",
                "instant": True,
                "intervalMs": 1000,
                "legendFormat": "__auto",
                "maxDataPoints": 43200,
                "range": False,
                "refId": "A",
            },
        },
        {
            "refId": "B",
            "relativeTimeRange": {"from": 600, "to": 0},
            "datasourceUid": "__expr__",
            "model": {
                "conditions": [
                    {
                        "evaluator": {"params": [0], "type": "gt"},
                        "operator": {"type": "and"},
                        "query": {"params": ["C"]},
                        "reducer": {"params": [], "type": "last"},
                        "type": "query",
                    }
                ],
                "datasource": {"type": "__expr__", "uid": "__expr__"},
                "expression": "A",
                "intervalMs": 1000,
                "maxDataPoints": 43200,
                "refId": "B",
                "type": "threshold",
            },
        },
    ],
    "noDataState": "NoData",
    "execErrState": "Error",
    "for": "5m",
    "annotations": {},
    "labels": {},
    "isPaused": False,
}
