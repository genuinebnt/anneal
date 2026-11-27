use solution::*;

#[test]
fn the_first_request_queues_the_job() {
    let job = QueueState::new();
    check!("first", job.request(), true);
    check!("already queued", job.request(), false);
    check!("still queued", job.request(), false);
}

#[test]
fn a_quiet_run_leaves_it_idle() {
    let job = QueueState::new();
    job.request();
    job.begin();
    check!("no request arrived during the run", job.end(), false);
    check!("so the next request queues it", job.request(), true);
}

#[test]
fn a_request_during_a_run_asks_for_another() {
    let job = QueueState::new();
    job.request();
    job.begin();
    check!("do not queue a running job", job.request(), false);
    check!("end: queue it again", job.end(), true);
    check!("it is queued now", job.request(), false);
}

#[test]
fn many_requests_during_a_run_give_one_more_run() {
    let job = QueueState::new();
    job.request();
    job.begin();
    job.request();
    job.request();
    job.request();
    check!("one more", job.end(), true);
    job.begin();
    check!("and then no more", job.end(), false);
}

#[test]
fn a_finished_job_is_never_queued_again() {
    let job = QueueState::new();
    job.request();
    job.begin();
    job.request();
    job.finish();
    check!("done", job.request(), false);
}

#[test]
#[should_panic]
fn begin_on_an_idle_job_panics() {
    QueueState::new().begin();
}
