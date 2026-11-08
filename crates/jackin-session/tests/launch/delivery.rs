// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{OutputStream, SandboxName, TerminalText};
use jackin_image::{BuildStage, ImageEvent};
use jackin_session::{
    LAUNCH_EVENT_CAPACITY, LaunchEvent, LaunchEvents, LaunchStep, LineDelivery, launch_events,
};

use super::support::{TestResult, guard, launch_rig, ready};
use jackin_testkit::{DockerBuildReply, Outcome};

const CAP: usize = LAUNCH_EVENT_CAPACITY;

fn line(index: usize) -> LaunchEvent {
    LaunchEvent::Image(ImageEvent::Line {
        stage: BuildStage::Role,
        text: TerminalText::sanitize(format!("line-{index}").as_bytes()),
    })
}

fn others() -> Result<[LaunchEvent; 5], Box<dyn std::error::Error>> {
    Ok([
        LaunchEvent::StepStarted(LaunchStep::Image),
        LaunchEvent::Image(ImageEvent::StageStarted(BuildStage::Role)),
        LaunchEvent::Image(ImageEvent::StageFinished(BuildStage::Role)),
        LaunchEvent::SandboxKnown(SandboxName::parse("jackin-work-abcd")?),
        LaunchEvent::Warning(jackin_session::LaunchWarning::DockerCapability),
    ])
}

async fn accepted(events: &LaunchEvents, event: LaunchEvent) -> bool {
    let emit = events.emit(event);
    tokio::select! {
        biased;
        () = emit => true,
        () = std::future::ready(()) => false,
    }
}

async fn collect(mut feed: jackin_session::LaunchFeed) -> Vec<LaunchEvent> {
    let mut seen = Vec::new();
    while let Some(event) = feed.recv().await {
        seen.push(event);
    }
    seen
}

#[tokio::test]
async fn reliable_delivery_applies_back_pressure_to_lines_and_loses_none() -> TestResult {
    guard(async {
        let (events, feed) = launch_events(LineDelivery::Reliable);
        for index in 0..CAP {
            assert!(accepted(&events, line(index)).await, "line {index}");
        }
        assert!(!accepted(&events, line(CAP)).await);
        let produce = async move {
            for index in CAP..4 * CAP {
                events.emit(line(index)).await;
            }
        };
        let (received, ()) = tokio::join!(collect(feed), produce);
        let expected: Vec<LaunchEvent> = (0..4 * CAP).map(line).collect();
        assert_eq!(received, expected);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn best_effort_delivery_drops_only_lines_when_full_and_keeps_order() -> TestResult {
    guard(async {
        let (events, feed) = launch_events(LineDelivery::BestEffort);
        for index in 0..CAP {
            assert!(accepted(&events, line(index)).await);
        }
        for index in CAP..3 * CAP {
            assert!(accepted(&events, line(index)).await, "line {index} waited");
        }
        let nonlines = others()?;
        for event in nonlines.clone() {
            assert!(!accepted(&events, event).await);
        }
        let emitted = [
            nonlines[0].clone(),
            line(3 * CAP),
            nonlines[1].clone(),
            line(3 * CAP + 1),
            nonlines[2].clone(),
            nonlines[3].clone(),
            nonlines[4].clone(),
            line(3 * CAP + 2),
        ];
        let produce = async {
            for event in emitted.clone() {
                events.emit(event).await;
            }
            drop(events);
        };
        let (received, ()) = tokio::join!(collect(feed), produce);
        let first: Vec<LaunchEvent> = (0..CAP).map(line).collect();
        assert_eq!(received[..CAP], first[..]);
        let kept: Vec<&LaunchEvent> = received
            .iter()
            .filter(|event| !matches!(event, LaunchEvent::Image(ImageEvent::Line { .. })))
            .collect();
        assert_eq!(kept, nonlines.iter().collect::<Vec<_>>());
        let mut last = None;
        for event in &received[CAP..] {
            let position = emitted.iter().position(|candidate| candidate == event);
            let position = position.ok_or_else(|| format!("{event:?} was never emitted"))?;
            assert!(last < Some(position), "{event:?} is out of order");
            last = Some(position);
        }
        Ok(())
    })
    .await
}

fn is_line(event: &LaunchEvent) -> bool {
    matches!(event, LaunchEvent::Image(ImageEvent::Line { .. }))
}

async fn slow_launch(
    delivery: LineDelivery,
    lines: &[String],
) -> Result<Vec<LaunchEvent>, Box<dyn std::error::Error>> {
    let rig = launch_rig().await?;
    let (_, builds, _) = super::support::actual_final_builds(&rig, 1).await?;
    rig.world.docker.script_build(
        builds.role.tag.clone(),
        DockerBuildReply::default().with_lines(
            lines
                .iter()
                .cloned()
                .map(|line| (OutputStream::Stderr, line))
                .collect(),
        ),
    );
    let plan = rig.plan(1)?;
    let (events, mut feed) = launch_events(delivery);
    let consume = async {
        let mut seen = Vec::new();
        while let Some(event) = feed.recv().await {
            seen.push(event);
            for _ in 0..32 {
                tokio::task::yield_now().await;
            }
        }
        seen
    };
    let (result, seen) = tokio::join!(
        rig.world
            .session
            .launch(plan, events, tokio_util::sync::CancellationToken::new()),
        consume
    );
    ready(Outcome {
        result,
        events: Vec::new(),
    })?;
    Ok(seen)
}

#[tokio::test]
async fn build_lines_reach_the_feed_through_launch_by_the_chosen_delivery() -> TestResult {
    guard(async {
        let lines: Vec<String> = (0..3 * CAP + 7).map(|n| format!("step {n:05}")).collect();
        let reliable = slow_launch(LineDelivery::Reliable, &lines).await?;
        let texts: Vec<&str> = reliable
            .iter()
            .filter_map(|event| match event {
                LaunchEvent::Image(ImageEvent::Line {
                    stage: BuildStage::Role,
                    text,
                }) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, lines.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(
            reliable.iter().filter(|event| is_line(event)).count(),
            lines.len()
        );
        let started = reliable
            .iter()
            .position(|event| {
                *event == LaunchEvent::Image(ImageEvent::StageStarted(BuildStage::Role))
            })
            .ok_or("no role start")?;
        let finished = reliable
            .iter()
            .position(|event| {
                *event == LaunchEvent::Image(ImageEvent::StageFinished(BuildStage::Role))
            })
            .ok_or("no role finish")?;
        let image_start = reliable
            .iter()
            .position(|event| *event == LaunchEvent::StepStarted(LaunchStep::Image))
            .ok_or("no image step")?;
        let image_end = reliable
            .iter()
            .position(|event| {
                matches!(
                    event,
                    LaunchEvent::StepFinished {
                        step: LaunchStep::Image,
                        ..
                    }
                )
            })
            .ok_or("no image end")?;
        let positions: Vec<usize> = reliable
            .iter()
            .enumerate()
            .filter(|(_, event)| is_line(event))
            .map(|(index, _)| index)
            .collect();
        assert!(positions.iter().all(|at| *at > started && *at < finished));
        assert!(image_start < started && finished < image_end);

        let best = slow_launch(LineDelivery::BestEffort, &lines).await?;
        let steady = |events: &[LaunchEvent]| -> Vec<LaunchEvent> {
            events
                .iter()
                .filter(|event| !is_line(event))
                .cloned()
                .collect()
        };
        assert_eq!(steady(&best), steady(&reliable));
        let kept: Vec<&str> = best
            .iter()
            .filter_map(|event| match event {
                LaunchEvent::Image(ImageEvent::Line { text, .. }) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            !kept.is_empty() && kept.len() < lines.len(),
            "{} lines",
            kept.len()
        );
        let indexes: Vec<usize> = kept
            .iter()
            .map(|text| text.trim_start_matches("step ").parse::<usize>())
            .collect::<Result<_, _>>()?;
        assert!(indexes.windows(2).all(|pair| pair[0] < pair[1]));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_dropped_feed_never_blocks_the_emitter() -> TestResult {
    guard(async {
        for delivery in [LineDelivery::Reliable, LineDelivery::BestEffort] {
            let (events, feed) = launch_events(delivery);
            for index in 0..CAP {
                assert!(accepted(&events, line(index)).await);
            }
            let waiting = async {
                events
                    .emit(LaunchEvent::StepStarted(LaunchStep::Image))
                    .await;
                events.emit(line(CAP)).await;
            };
            let closing = async move {
                tokio::task::yield_now().await;
                drop(feed);
            };
            tokio::join!(waiting, closing);
            for event in others()? {
                assert!(accepted(&events, event).await);
            }
            assert!(accepted(&events, line(CAP + 1)).await);
        }
        Ok(())
    })
    .await
}
