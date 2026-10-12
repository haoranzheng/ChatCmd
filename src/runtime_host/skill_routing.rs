//! Explainable, advisory skill recommendations and observed read events.
use chatcmd_core::{TaskId, TurnId};
use chatcmd_runtime::{OperationContext, SkillSummary};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

fn words(s: &str) -> BTreeSet<String> {
    s.to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric() && ch != '-')
        .filter(|s| {
            s.chars().count() >= 4
                && !matches!(
                    *s,
                    "with"
                        | "from"
                        | "this"
                        | "that"
                        | "your"
                        | "task"
                        | "skills"
                        | "skill"
                        | "using"
                        | "about"
                        | "when"
                        | "code"
                        | "tool"
                        | "tools"
                        | "make"
                        | "create"
                        | "project"
                )
        })
        .map(str::to_owned)
        .collect()
}
const SYNONYMS: &[(&str, &str)] = &[
    ("蓝图", "blueprint"),
    ("虚幻", "unreal"),
    ("引擎", "unreal"),
    ("调试", "debug"),
    ("排查", "diagnos"),
    ("故障", "diagnos"),
    ("界面", "frontend"),
    ("前端", "frontend"),
    ("网页", "frontend"),
    ("测试", "test"),
    ("安全", "security"),
    ("漏洞", "security"),
    ("数据库", "database"),
    ("性能", "performance"),
    ("部署", "deploy"),
    ("材质", "material"),
    ("动画", "animation"),
];
pub(crate) fn rank(message: &str, skills: &[SkillSummary]) -> Vec<Value> {
    if message.trim().chars().count() < 4 {
        return vec![];
    }
    let message = message.to_lowercase();
    let terms = words(&message);
    let mut matches = skills
        .iter()
        .filter_map(|s| {
            let name = s.name.to_lowercase();
            let detail = s.description.to_lowercase();
            let mut points = 0_i32;
            let mut reasons = BTreeSet::new();
            for word in &terms {
                if name.contains(word) {
                    points += 6;
                    reasons.insert(word.clone());
                } else if detail
                    .split(|ch: char| !ch.is_alphanumeric())
                    .any(|x| x == word)
                {
                    points += 2;
                    reasons.insert(word.clone());
                }
            }
            for (term, keyword) in SYNONYMS {
                if message.contains(term) && (name.contains(keyword) || detail.contains(keyword)) {
                    points += 4;
                    reasons.insert((*term).to_owned());
                }
            }
            if points < 4 {
                return None;
            }
            let reason = format!(
                "任务关键词匹配：{}",
                reasons.into_iter().take(4).collect::<Vec<_>>().join("、")
            );
            Some((
                points,
                name,
                json!({"skillId":s.id,"title":s.title,
            "reason":reason,"score":points}),
            ))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    matches.into_iter().take(3).map(|(_, _, v)| v).collect()
}
fn stable_id(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for part in parts {
        h.update(part.as_bytes());
        h.update([0]);
    }
    format!("skill-route-{:x}", h.finalize())
}
async fn append(
    pool: &SqlitePool,
    task: &str,
    turn: &str,
    key: &str,
    payload: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR IGNORE INTO timeline_events(event_id,task_id,turn_id,session_id,actor,kind,idempotency_key,payload_json,metadata_json,created_at_ms) VALUES(?,?,?,NULL,'system','status',?,?,NULL,?)")
      .bind(key).bind(task).bind(turn).bind(key).bind(payload.to_string())
      .bind(crate::runtime_host::now_ms()).execute(pool).await?;
    Ok(())
}
pub(crate) async fn record_recommendations(
    pool: &SqlitePool,
    task: &TaskId,
    turn: &TurnId,
    recommendations: &[Value],
) -> Result<(), sqlx::Error> {
    let data =
        json!({"source":"skill_router","eventType":"suggested","suggestions":recommendations});
    append(
        pool,
        task.as_str(),
        turn.as_str(),
        &stable_id(&["suggested", task.as_str(), turn.as_str()]),
        &data,
    )
    .await
}
pub(crate) async fn record_read(
    pool: &SqlitePool,
    context: &OperationContext,
    skill: &str,
    source: &str,
    truncated: bool,
) -> Result<(), sqlx::Error> {
    let (Some(task), Some(turn)) = (context.task_id.as_deref(), context.turn_id.as_deref()) else {
        return Ok(());
    };
    let data = json!({"source":"skill_router","eventType":"read","skillId":skill,
        "skillSource":source,"truncated":truncated});
    append(
        pool,
        task,
        turn,
        &stable_id(&["read", task, turn, skill]),
        &data,
    )
    .await
}
#[cfg(test)]
mod tests {
    use super::*;
    fn skill(name: &str, desc: &str) -> SkillSummary {
        SkillSummary {
            id: name.into(),
            name: name.into(),
            title: name.into(),
            description: desc.into(),
            source: "/tmp/skill".into(),
        }
    }
    #[test]
    fn ranking_is_explainable_and_conservative() {
        let skills = [
            skill("blueprint-flow", "Unreal Blueprint execution flow"),
            skill("frontend", "Frontend interface work"),
        ];
        let s = rank("检查蓝图执行链路", &skills);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0]["skillId"], "blueprint-flow");
        assert!(s[0]["reason"].as_str().unwrap().contains("蓝图"));
        assert!(rank("你好，继续", &skills).is_empty());
    }
    #[test]
    fn event_keys_are_idempotent() {
        assert_eq!(
            stable_id(&["read", "t", "a", "x"]),
            stable_id(&["read", "t", "a", "x"])
        );
        assert_ne!(
            stable_id(&["read", "t", "a", "x"]),
            stable_id(&["read", "t", "b", "x"])
        );
    }
}
