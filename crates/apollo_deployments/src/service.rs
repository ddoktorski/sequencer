use std::fmt::Display;
use std::path::PathBuf;

use apollo_node::config::component_config::ComponentConfig;
use indexmap::IndexMap;
use serde::{Serialize, Serializer};
use strum::{Display, EnumVariantNames, IntoEnumIterator};
use strum_macros::{EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::deployment_definitions::Environment;
use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;

const INGRESS_ROUTE: &str = "/gateway";
const INGRESS_PORT: u16 = 8080;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Service {
    name: ServiceName,
    // TODO(Tsabary): change config path to PathBuf type.
    controller: Controller,
    config_paths: Vec<String>,
    ingress: Option<Ingress>,
    autoscale: bool,
    replicas: usize,
    storage: Option<usize>,
    toleration: Option<Toleration>,
    resources: Resources,
    external_secret: Option<ExternalSecret>,
    #[serde(skip_serializing)]
    environment: Environment,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Controller {
    Deployment,
    StatefulSet,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Ingress {
    #[serde(flatten)]
    ingress_params: IngressParams,
    internal: bool,
    rules: Vec<IngressRule>,
}

impl Ingress {
    pub fn new(ingress_params: IngressParams, internal: bool, rules: Vec<IngressRule>) -> Self {
        Self { ingress_params, internal, rules }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IngressParams {
    domain: String,
    #[serde(serialize_with = "serialize_none_as_empty_vec")]
    alternative_names: Option<Vec<String>>,
}

fn serialize_none_as_empty_vec<S, T>(
    value: &Option<Vec<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    match value {
        Some(v) => serializer.serialize_some(v),
        None => serializer.serialize_some(&Vec::<T>::new()),
    }
}

impl IngressParams {
    pub fn new(domain: String, alternative_names: Option<Vec<String>>) -> Self {
        Self { domain, alternative_names }
    }
}

pub(crate) fn get_ingress(ingress_params: IngressParams, internal: bool) -> Option<Ingress> {
    Some(Ingress::new(
        ingress_params,
        internal,
        vec![IngressRule::new(String::from(INGRESS_ROUTE), INGRESS_PORT, None)],
    ))
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IngressRule {
    path: String,
    port: u16,
    backend: Option<String>,
}

impl IngressRule {
    pub fn new(path: String, port: u16, backend: Option<String>) -> Self {
        Self { path, port, backend }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExternalSecret {
    gcsm_key: &'static str,
}

impl ExternalSecret {
    pub fn new(gcsm_key: &'static str) -> Self {
        Self { gcsm_key }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Resource {
    cpu: usize,
    memory: usize,
}

impl Resource {
    pub fn new(cpu: usize, memory: usize) -> Self {
        Self { cpu, memory }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Resources {
    requests: Resource,
    limits: Resource,
}

impl Resources {
    pub fn new(requests: Resource, limits: Resource) -> Self {
        Self { requests, limits }
    }
}

// TODO(Tsabary): remove clippy::too_many_arguments
impl Service {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: ServiceName,
        replicas: usize,
        storage: Option<usize>,
        resources: Resources,
        external_secret: Option<ExternalSecret>,
        mut additional_config_filenames: Vec<String>,
        ingress_params: IngressParams,
        // TODO(Tsabary): consider if including the environment is necessary.
        environment: Environment,
    ) -> Self {
        // Configs are loaded by order such that a config may override previous ones.
        // We first list the base config, and then follow with the overrides.
        // TODO(Tsabary): the service override is currently engrained in the base config, need to
        // resolve that.
        let mut config_paths: Vec<String> = vec![name.get_config_file_path()];
        config_paths.append(&mut additional_config_filenames);

        let controller = name.get_controller();
        let autoscale = name.get_autoscale();
        let toleration = name.get_toleration(&environment);
        let ingress = name.get_ingress(&environment, ingress_params);
        Self {
            name,
            config_paths,
            controller,
            ingress,
            autoscale,
            replicas,
            storage,
            toleration,
            resources,
            external_secret,
            environment,
        }
    }

    pub fn get_config_paths(&self) -> Vec<String> {
        self.config_paths.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumDiscriminants)]
#[strum_discriminants(
    name(DeploymentName),
    derive(IntoStaticStr, EnumIter, EnumVariantNames, Serialize, Display),
    strum(serialize_all = "snake_case")
)]
pub enum ServiceName {
    ConsolidatedNode(ConsolidatedNodeServiceName),
    HybridNode(HybridNodeServiceName),
    DistributedNode(DistributedNodeServiceName),
}

impl ServiceName {
    pub fn get_config_file_path(&self) -> String {
        let mut name = self.as_inner().to_string();
        name.push_str(".json");
        name
    }

    pub fn create_service(
        &self,
        environment: &Environment,
        external_secret: &Option<ExternalSecret>,
        additional_config_filenames: Vec<String>,
        ingress_params: IngressParams,
    ) -> Service {
        self.as_inner().create_service(
            environment,
            external_secret,
            additional_config_filenames,
            ingress_params,
        )
    }

    fn as_inner(&self) -> &dyn ServiceNameInner {
        match self {
            ServiceName::ConsolidatedNode(inner) => inner,
            ServiceName::HybridNode(inner) => inner,
            ServiceName::DistributedNode(inner) => inner,
        }
    }

    pub fn get_controller(&self) -> Controller {
        self.as_inner().get_controller()
    }

    pub fn get_autoscale(&self) -> bool {
        self.as_inner().get_autoscale()
    }

    pub fn get_toleration(&self, environment: &Environment) -> Option<Toleration> {
        self.as_inner().get_toleration(environment)
    }

    pub fn get_ingress(
        &self,
        environment: &Environment,
        ingress_params: IngressParams,
    ) -> Option<Ingress> {
        self.as_inner().get_ingress(environment, ingress_params)
    }
}

pub(crate) trait ServiceNameInner: Display {
    fn create_service(
        &self,
        environment: &Environment,
        external_secret: &Option<ExternalSecret>,
        additional_config_filenames: Vec<String>,
        ingress_params: IngressParams,
    ) -> Service;

    fn get_controller(&self) -> Controller;

    fn get_autoscale(&self) -> bool;

    fn get_toleration(&self, environment: &Environment) -> Option<Toleration>;

    fn get_ingress(
        &self,
        environment: &Environment,
        ingress_params: IngressParams,
    ) -> Option<Ingress>;
}

impl DeploymentName {
    pub fn add_path_suffix(&self, path: PathBuf, instance_name: &str) -> PathBuf {
        let deployment_name_dir = match self {
            // TODO(Tsabary): find a way to avoid this code duplication.
            // Trailing backslash needed to mitigate deployment test issues.
            Self::ConsolidatedNode => path.join("consolidated/"),
            Self::HybridNode => path.join("hybrid/"),
            Self::DistributedNode => path.join("distributed/"),
        };
        println!("Deployment name dir: {:?}", deployment_name_dir);
        let deployment_with_instance = deployment_name_dir.join(instance_name);
        println!("Deployment with instance: {:?}", deployment_with_instance);

        let s = deployment_with_instance.to_string_lossy();
        let modified = if s.ends_with('/') { s.into_owned() } else { format!("{}/", s) };
        modified.into()
    }

    pub fn all_service_names(&self) -> Vec<ServiceName> {
        match self {
            // TODO(Tsabary): find a way to avoid this code duplication.
            Self::ConsolidatedNode => {
                ConsolidatedNodeServiceName::iter().map(ServiceName::ConsolidatedNode).collect()
            }
            Self::HybridNode => {
                HybridNodeServiceName::iter().map(ServiceName::HybridNode).collect()
            }
            Self::DistributedNode => {
                DistributedNodeServiceName::iter().map(ServiceName::DistributedNode).collect()
            }
        }
    }

    pub fn get_component_configs(
        &self,
        base_port: Option<u16>,
        environment: &Environment,
    ) -> IndexMap<ServiceName, ComponentConfig> {
        match self {
            // TODO(Tsabary): avoid this code duplication.
            Self::ConsolidatedNode => {
                ConsolidatedNodeServiceName::get_component_configs(base_port, environment)
            }
            Self::HybridNode => {
                HybridNodeServiceName::get_component_configs(base_port, environment)
            }
            Self::DistributedNode => {
                DistributedNodeServiceName::get_component_configs(base_port, environment)
            }
        }
    }
}

pub trait GetComponentConfigs {
    // TODO(Tsabary): replace IndexMap with regular HashMap. Currently using IndexMap as the
    // integration test relies on indices rather than service names.
    fn get_component_configs(
        base_port: Option<u16>,
        environment: &Environment,
    ) -> IndexMap<ServiceName, ComponentConfig>;
}

impl Serialize for ServiceName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize only the inner value.
        match self {
            ServiceName::ConsolidatedNode(inner) => inner.serialize(serializer),
            ServiceName::HybridNode(inner) => inner.serialize(serializer),
            ServiceName::DistributedNode(inner) => inner.serialize(serializer),
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Toleration {
    ApolloCoreService,
    ApolloGeneralService,
}
