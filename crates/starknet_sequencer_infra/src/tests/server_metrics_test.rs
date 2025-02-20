use std::convert::TryInto;
use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use metrics::set_default_local_recorder;
use metrics_exporter_prometheus::PrometheusBuilder;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{channel, Receiver};
use tokio::sync::Semaphore;
use tokio::task::{self, JoinSet};

use crate::component_client::{ClientResult, LocalComponentClient};
use crate::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
    ComponentStarter,
};
use crate::component_server::{
    ComponentServerStarter,
    ConcurrentLocalComponentServer,
    LocalComponentServer,
};
use crate::tests::TEST_LOCAL_SERVER_METRICS;

type TestResult = ClientResult<()>;

const NUMBER_OF_ITERATIONS: usize = 10;

#[derive(Serialize, Deserialize, Debug)]
enum TestComponentRequest {
    PerformTest,
}

#[derive(Serialize, Deserialize, Debug)]
enum TestComponentResponse {
    PerformTest,
}

type LocalTestComponentClient = LocalComponentClient<TestComponentRequest, TestComponentResponse>;
type TestReceiver =
    Receiver<ComponentRequestAndResponseSender<TestComponentRequest, TestComponentResponse>>;

#[async_trait]
trait TestComponentClientTrait: Send + Sync {
    async fn perform_test(&self) -> TestResult;
}

#[derive(Clone)]
struct TestComponent {
    test_sem: Arc<Semaphore>,
}

impl TestComponent {
    pub fn new(test_sem: Arc<Semaphore>) -> Self {
        Self { test_sem }
    }

    pub async fn reduce_permit(&self) {
        self.test_sem.acquire().await.unwrap().forget();
    }
}

impl ComponentStarter for TestComponent {}

#[async_trait]
impl ComponentRequestHandler<TestComponentRequest, TestComponentResponse> for TestComponent {
    async fn handle_request(&mut self, request: TestComponentRequest) -> TestComponentResponse {
        match request {
            TestComponentRequest::PerformTest => {
                self.reduce_permit().await;
                TestComponentResponse::PerformTest
            }
        }
    }
}

#[async_trait]
impl<ComponentClientType> TestComponentClientTrait for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<TestComponentRequest, TestComponentResponse>,
{
    async fn perform_test(&self) -> TestResult {
        match self.send(TestComponentRequest::PerformTest).await? {
            TestComponentResponse::PerformTest => Ok(()),
        }
    }
}

struct BasicSetup {
    component: TestComponent,
    local_client: LocalTestComponentClient,
    rx: TestReceiver,
    test_sem: Arc<Semaphore>,
}

fn basic_test_setup() -> BasicSetup {
    let test_sem = Arc::new(Semaphore::new(0));
    let component = TestComponent::new(test_sem.clone());

    let (tx, rx) = channel::<
        ComponentRequestAndResponseSender<TestComponentRequest, TestComponentResponse>,
    >(32);

    let local_client = LocalTestComponentClient::new(tx);

    BasicSetup { component, local_client, rx, test_sem }
}

async fn setup_local_server_test() -> (Arc<Semaphore>, LocalTestComponentClient) {
    let BasicSetup { component, local_client, rx, test_sem } = basic_test_setup();

    let max_concurrency = 1;
    let mut local_server =
        LocalComponentServer::new(component, rx, max_concurrency, TEST_LOCAL_SERVER_METRICS);
    task::spawn(async move {
        let _ = local_server.start().await;
    });

    (test_sem, local_client)
}

async fn setup_concurrent_local_server_test(
    max_concurrency: usize,
) -> (Arc<Semaphore>, LocalTestComponentClient) {
    let BasicSetup { component, local_client, rx, test_sem } = basic_test_setup();

    let mut concurrent_local_server = ConcurrentLocalComponentServer::new(
        component,
        rx,
        max_concurrency,
        TEST_LOCAL_SERVER_METRICS,
    );
    task::spawn(async move {
        let _ = concurrent_local_server.start().await;
    });

    (test_sem, local_client)
}

fn usize_to_u64(value: usize) -> u64 {
    value.try_into().expect("Conversion failed")
}

fn assert_server_metrics(
    metrics_as_string: &str,
    expected_received_msgs: usize,
    expected_processed_msgs: usize,
    expected_queue_depth: usize,
) {
    let received_msgs = TEST_LOCAL_SERVER_METRICS.get_received_value(metrics_as_string);
    let processed_msgs = TEST_LOCAL_SERVER_METRICS.get_processed_value(metrics_as_string);
    let queue_depth = TEST_LOCAL_SERVER_METRICS.get_queue_depth_value(metrics_as_string);

    assert_eq!(
        received_msgs,
        Some(usize_to_u64(expected_received_msgs)),
        "unexpected value for receives_msgs_started counter, expected {} got {:?}",
        expected_received_msgs,
        received_msgs,
    );
    assert_eq!(
        processed_msgs,
        Some(usize_to_u64(expected_processed_msgs)),
        "unexpected value for processed_msgs counter, expected {} got {:?}",
        expected_processed_msgs,
        processed_msgs,
    );
    assert_eq!(
        queue_depth,
        Some(expected_queue_depth),
        "unexpected value for queue_depth, expected {} got {:?}",
        expected_queue_depth,
        queue_depth,
    );
}

#[tokio::test]
async fn only_metrics_counters_for_local_server() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let (test_sem, client) = setup_local_server_test().await;

    // At the beginning all metrics counters are zero.
    let metrics_as_string = recorder.handle().render();
    assert_server_metrics(metrics_as_string.as_str(), 0, 0, 0);

    // In order to process a message the test component tries to acquire a permit from the
    // test semaphore. Current test is checking that all metrics counters actually count so we
    // need to provide enough permits for all messages to be processed.
    test_sem.add_permits(NUMBER_OF_ITERATIONS);
    for i in 0..NUMBER_OF_ITERATIONS {
        client.perform_test().await.unwrap();

        // Every time the request is sent and the response is received the metrics counters should
        // be increased by one.
        let metrics_as_string = recorder.handle().render();
        assert_server_metrics(metrics_as_string.as_str(), i + 1, i + 1, 0);
    }
}

#[tokio::test]
async fn all_metrics_for_local_server() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let (test_sem, client) = setup_local_server_test().await;

    // In order to test not only message counters but the queue depth too, first we will send all
    // the messages by spawning multiple clients and by that filling the channel queue.
    for _ in 0..NUMBER_OF_ITERATIONS {
        let multi_client = client.clone();
        task::spawn(async move {
            multi_client.perform_test().await.unwrap();
        });
    }
    task::yield_now().await;

    // And then we will provide a single permit each time and check that all metrics are adjusted
    // accordingly.
    for i in 0..NUMBER_OF_ITERATIONS {
        let metrics_as_string = recorder.handle().render();
        // After sending i permits we should have i + 1 received messages, because the first message
        // doesn't need a permit to be received but need a permit to be processed.
        // So we will have only i processed messages.
        // And the queue depth should be: NUMBER_OF_ITERATIONS - number of received messages.
        assert_server_metrics(metrics_as_string.as_str(), i + 1, i, NUMBER_OF_ITERATIONS - i - 1);
        test_sem.add_permits(1);
        task::yield_now().await;
    }

    // Finally all messages processed and queue is empty.
    let metrics_as_string = recorder.handle().render();
    assert_server_metrics(
        metrics_as_string.as_str(),
        NUMBER_OF_ITERATIONS,
        NUMBER_OF_ITERATIONS,
        0,
    );
}

#[tokio::test]
async fn only_metrics_counters_for_concurrent_server() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = set_default_local_recorder(&recorder);

    let max_concurrency = NUMBER_OF_ITERATIONS;
    let (test_sem, client) = setup_concurrent_local_server_test(max_concurrency).await;

    // Current test is checking that all metrics counters can actually count in parallel.
    // So first we send all the messages.
    let mut tasks = JoinSet::new();
    for _ in 0..NUMBER_OF_ITERATIONS {
        let multi_client = client.clone();
        tasks.spawn(async move {
            multi_client.perform_test().await.unwrap();
        });
    }
    task::yield_now().await;

    // By now all messages should be received but not processed.
    let metrics_as_string = recorder.handle().render();
    assert_server_metrics(metrics_as_string.as_str(), NUMBER_OF_ITERATIONS, 0, 0);

    // Now we provide all permits and wait for all messages to be processed.
    test_sem.add_permits(NUMBER_OF_ITERATIONS);
    tasks.join_all().await;

    // Finally all messages processed and queue is empty.
    let metrics_as_string = recorder.handle().render();
    assert_server_metrics(
        metrics_as_string.as_str(),
        NUMBER_OF_ITERATIONS,
        NUMBER_OF_ITERATIONS,
        0,
    );
}
