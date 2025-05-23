#!/usr/bin/env python3

import sys

from constructs import Construct
from cdk8s import App, Chart, YamlOutputType

from app.service import ServiceApp
from app.monitoring import MonitoringApp
from services import topology, helpers, config, monitoring


class SequencerNode(Chart):
    def __init__(
        self,
        scope: Construct,
        name: str,
        namespace: str,
        monitoring: bool,
        service_topology: topology.ServiceTopology,
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)
        self.service = ServiceApp(
            self,
            name,
            namespace=namespace,
            service_topology=service_topology,
            monitoring=monitoring,
        )


class SequencerMonitoring(Chart):
    def __init__(
        self,
        scope: Construct,
        name: str,
        cluster: str,
        namespace: str,
        grafana_dashboard: monitoring.GrafanaDashboard,
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)
        self.dashboard = MonitoringApp(
            self, name, cluster=cluster, namespace=namespace, grafana_dashboard=grafana_dashboard
        )


def main():
    args = helpers.argument_parser()

    assert not (
        args.monitoring_dashboard_file and not args.cluster
    ), "Error: --cluster is required when --monitoring-dashboard-file is provided."

    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)

    preset = config.DeploymentConfig(args.deployment_config_file)
    services = preset.get_services()
    image = f"ghcr.io/starkware-libs/sequencer/sequencer:{args.deployment_image_tag}"
    application_config_subdir = preset.get_application_config_subdir()
    create_monitoring = True if args.monitoring_dashboard_file else False

    for svc in services:
        SequencerNode(
            scope=app,
            name=helpers.sanitize_name(f'sequencer-{svc["name"]}'),
            namespace=helpers.sanitize_name(args.namespace),
            monitoring=create_monitoring,
            service_topology=topology.ServiceTopology(
                config=config.SequencerConfig(
                    config_subdir=application_config_subdir, config_paths=svc["config_paths"]
                ),
                image=image,
                controller=svc["controller"].lower(),
                replicas=svc["replicas"],
                autoscale=svc["autoscale"],
                ingress=svc["ingress"],
                storage=svc["storage"],
                toleration=svc["toleration"],
                resources=svc["resources"],
                external_secret=svc["external_secret"],
            ),
        )

    if args.monitoring_dashboard_file:
        dashboard_hash = helpers.generate_random_hash(
            from_string=f"{args.cluster}-{args.namespace}"
        )
        SequencerMonitoring(
            scope=app,
            name=helpers.sanitize_name(f"sequencer-monitoring-{dashboard_hash}"),
            cluster=args.cluster,
            namespace=helpers.sanitize_name(args.namespace),
            grafana_dashboard=monitoring.GrafanaDashboard(args.monitoring_dashboard_file),
        )

    app.synth()


if __name__ == "__main__":
    main()
