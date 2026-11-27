//! A course (courses/<id>): projects of modules of stages, loaded from `course.toml`, `lectures.toml`, `modules/*/module.toml`
//! and `modules/*/stages/*/{stage.toml,stage.md}`. The web app reads it through the API; `anneal course` has its own loader for
//! learner repos and keeps the same file formats.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum CourseError {
    #[error("{0}")]
    Invalid(String),
}

type Result<T> = std::result::Result<T, CourseError>;

fn bad<T>(m: impl Into<String>) -> Result<T> {
    Err(CourseError::Invalid(m.into()))
}

#[derive(Debug, Deserialize)]
struct CourseToml {
    id: String,
    title: String,
    #[serde(default)]
    project: Vec<ProjectToml>,
    /// Modules the web app shows. Unset: all of them. Modules ship one at a time, so a half-written one stays hidden.
    #[serde(default)]
    published_modules: Option<Vec<String>>,
    /// Modules still to be (re)written: shown with a "planned" tag and skipped when picking the next stage.
    #[serde(default)]
    planned_modules: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectToml {
    number: u32,
    title: String,
    /// Not written yet: shown in the tree as "planned".
    #[serde(default)]
    planned: bool,
}

#[derive(Debug, Deserialize)]
struct ModuleToml {
    code: String,
    title: String,
    /// The project (in `course.toml`'s `[[project]]` list) the module belongs to, when its code does not start with the project's number.
    #[serde(default)]
    project: Option<u32>,
    /// Not on the main path: skipped when picking the next stage (the learner opens it on purpose).
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    lectures: Vec<String>,
    #[serde(default)]
    bustub: Vec<String>,
    /// Beyond BusTub: nothing of the module is in BusTub's own projects. Defaults to "the module ports no BusTub file".
    #[serde(default)]
    beyond: Option<bool>,
    #[serde(default)]
    resources: Vec<Resource>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Resource {
    /// docs · book · paper · blog · video · code · man
    pub kind: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct LecturesToml {
    #[serde(default)]
    lecture: Vec<Lecture>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Lecture {
    pub id: String,
    pub title: String,
    pub term: String,
    pub slides: String,
    pub notes: Option<String>,
    pub video: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StageToml {
    id: String,
    title: String,
    kind: String,
    difficulty: String,
    tests: Vec<String>,
    /// Concept pages (courses/<id>/concepts/<id>.md) worth reading before or while doing this stage.
    #[serde(default)]
    concepts: Vec<String>,
    /// Further reading that goes beyond what the stage needs; `concepts` stays the required list.
    #[serde(default)]
    concepts_optional: Vec<String>,
    /// What the learner takes away: short phrases shown as "You'll learn" under the stage title.
    #[serde(default)]
    learn: Vec<String>,
    /// Not part of BusTub: an extra the course adds (a challenge is always beyond BusTub).
    #[serde(default)]
    beyond: bool,
    /// The stages this one replaces (see [`FormerStage`]).
    #[serde(default)]
    renamed_from: Vec<FormerStage>,
}

/// One entry of a stage's `renamed_from`: a stage that no longer exists under its id, whose stored progress this stage takes
/// over. A renamed or moved stage lists its old id; each stage of a split lists the stage it came from; a merged stage lists
/// every stage it merges. A stage that was renamed more than once is one entry with all its ids, newest first:
/// `renamed_from = ["2a-02", ["2a-03", "1c-05"]]` replaces `2a-02` and the stage once called `1c-05`, then `2a-03`. Renaming a
/// stage that itself replaced several keeps one entry per stage it replaced, each led by its newest id:
/// `[["2b-01", "2a-01"], ["2b-01", "2a-02"]]` is `2b-01`, which merged `2a-01` and `2a-02`.
///
/// Progress is stored by stage id and never moved: a stage with no stored state of its own reads its former stages'
/// (see [`former_states`]). A stage passes that way only when every former stage passed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum FormerStage {
    Id(String),
    Ids(Vec<String>),
}

impl FormerStage {
    /// The ids the stage had, newest first.
    pub fn ids(&self) -> &[String] {
        match self {
            FormerStage::Id(id) => std::slice::from_ref(id),
            FormerStage::Ids(ids) => ids,
        }
    }
}

/// Every id in a `renamed_from` list.
pub fn former_ids(former: &[FormerStage]) -> impl Iterator<Item = &str> {
    former.iter().flat_map(|f| f.ids()).map(String::as_str)
}

/// The stored state of each former stage, in `renamed_from` order: the state `stored` returns for the newest of the stage's ids
/// that has one, or `None` when none of them has.
pub fn former_states<T>(former: &[FormerStage], mut stored: impl FnMut(&str) -> Option<T>) -> Vec<Option<T>> {
    former.iter().map(|f| f.ids().iter().find_map(|id| stored(id))).collect()
}

/// Whether a stage with no stored pass of its own counts as passed: it replaces at least one stage and every one of them passed.
pub fn passed_through_former(former: &[FormerStage], passed: impl Fn(&str) -> bool) -> bool {
    !former.is_empty() && former_states(former, |id| passed(id).then_some(())).iter().all(Option::is_some)
}

/// Checks the `renamed_from` lists of a course's stages, given as `(stage id, renamed_from)`. A former id must not be a current
/// stage id (its progress would count twice), an entry names each id once and no entry is repeated. Several stages may list the
/// same former stage: the parts of a split each take over the stage they came from.
pub fn check_renamed_from<'a>(stages: impl IntoIterator<Item = (&'a str, &'a [FormerStage])> + Clone) -> std::result::Result<(), String> {
    let current: std::collections::HashSet<&str> = stages.clone().into_iter().map(|(id, _)| id).collect();
    for (id, former) in stages {
        for (i, f) in former.iter().enumerate() {
            if f.ids().is_empty() {
                return Err(format!("stage {id}: an empty list in renamed_from"));
            }
            if former[..i].iter().any(|e| e.ids() == f.ids()) {
                return Err(format!("stage {id}: renamed_from lists {:?} twice", f.ids()));
            }
            for (j, old) in f.ids().iter().enumerate() {
                if current.contains(old.as_str()) {
                    return Err(format!("stage {id}: renamed_from {old}: that id is a current stage"));
                }
                if f.ids()[..j].contains(old) {
                    return Err(format!("stage {id}: renamed_from names {old} twice in one entry"));
                }
            }
        }
    }
    Ok(())
}

/// One `## Heading` of a stage's markdown, with its body.
#[derive(Debug, Clone, Serialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub md: String,
}

/// One `### Heading` under a stage's `## Hints`. Hints are written for the stage, deepest last: a nudge about the design,
/// then the trap, then the invariant to check. Shown one at a time and counted as assistance.
#[derive(Debug, Clone, Serialize)]
pub struct Hint {
    pub title: String,
    pub md: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stage {
    pub concepts: Vec<String>,
    /// Concepts that are good to read but not needed for the stage.
    pub concepts_optional: Vec<String>,
    pub learn: Vec<String>,
    /// Not part of BusTub's own projects: a challenge, or a stage the course adds (`beyond = true` in stage.toml).
    pub beyond: bool,
    pub id: String,
    pub title: String,
    /// learn · build · boss
    pub kind: String,
    /// very-easy · easy · medium · hard
    pub difficulty: String,
    pub tests: Vec<String>,
    /// The stages this one replaces: their stored progress counts for it until it has its own.
    #[serde(skip)]
    pub renamed_from: Vec<FormerStage>,
    pub module: String,
    /// 1-based position in the whole course.
    pub rank: usize,
    /// The markdown before the first `##`.
    pub intro: String,
    pub sections: Vec<Section>,
    pub hints: Vec<Hint>,
    /// The source of this stage's tests by test name, from the template's test files (shown next to the results of a run).
    #[serde(skip)]
    pub test_sources: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Module {
    pub code: String,
    /// Not (re)written yet: the stages shown are the old ones and will change.
    pub planned: bool,
    /// Not on the main path: skipped when picking the next stage.
    pub optional: bool,
    pub title: String,
    pub summary: String,
    pub project: u32,
    pub lectures: Vec<Lecture>,
    /// BusTub's own files this module ports, as GitHub URLs.
    pub bustub: Vec<String>,
    /// Not part of BusTub: the module ports none of its files (or says `beyond = true`). The web app tags it.
    pub beyond: bool,
    pub resources: Vec<Resource>,
    pub stages: Vec<Stage>,
}

/// A short article that teaches one idea a stage needs (courses/<id>/concepts/<id>.md). A header between `---` lines sets
/// `title`, `summary` and `minutes`; each `## ` heading starts a section.
#[derive(Debug, Clone, Serialize)]
pub struct Concept {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub minutes: u32,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub number: u32,
    pub title: String,
    pub planned: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Course {
    pub id: String,
    pub title: String,
    pub projects: Vec<Project>,
    pub modules: Vec<Module>,
    pub concepts: Vec<Concept>,
}

impl Course {
    /// The course as the web app shows it: only the modules in `published_modules`, if that is set.
    pub fn load(root: &Path) -> Result<Course> {
        Course::load_with(root, true)
    }

    /// Every module, published or not (for `anneal course lint --all`).
    pub fn load_all(root: &Path) -> Result<Course> {
        Course::load_with(root, false)
    }

    fn load_with(root: &Path, only_published: bool) -> Result<Course> {
        let meta: CourseToml = read_toml(&root.join("course.toml"))?;
        let lectures: Vec<Lecture> = match fs::read_to_string(root.join("lectures.toml")) {
            Ok(t) => toml::from_str::<LecturesToml>(&t).map_err(|e| CourseError::Invalid(format!("lectures.toml: {e}")))?.lecture,
            Err(_) => Vec::new(),
        };
        let mut modules = Vec::new();
        let mut rank = 0;
        let mut seen: HashMap<String, usize> = HashMap::new();
        for dir in sorted_dirs(&root.join("modules"))? {
            let mt: ModuleToml = read_toml(&dir.join("module.toml"))?;
            let project: u32 = match mt.project {
                Some(p) => p,
                None => mt.code.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().map_err(|_| {
                    CourseError::Invalid(format!("module code {:?} must start with its project number, like 2b (or set `project` in module.toml)", mt.code))
                })?,
            };
            let mut mod_lectures = Vec::new();
            for id in &mt.lectures {
                match lectures.iter().find(|l| &l.id == id) {
                    Some(l) => mod_lectures.push(l.clone()),
                    None => return bad(format!("module {}: no lecture {id:?} in lectures.toml", mt.code)),
                }
            }
            let mut stages = Vec::new();
            for sdir in sorted_dirs(&dir.join("stages"))? {
                let def: StageToml = read_toml(&sdir.join("stage.toml"))?;
                let md = fs::read_to_string(sdir.join("stage.md"))
                    .map_err(|e| CourseError::Invalid(format!("{}: reading stage.md: {e}", sdir.display())))?;
                if !matches!(def.kind.as_str(), "learn" | "build" | "boss" | "challenge") {
                    return bad(format!("{}: kind must be learn, build, boss or challenge", def.id));
                }
                if !matches!(def.difficulty.as_str(), "very-easy" | "easy" | "medium" | "hard") {
                    return bad(format!("{}: difficulty must be very-easy, easy, medium or hard", def.id));
                }
                rank += 1;
                if let Some(prev) = seen.insert(def.id.clone(), rank) {
                    return bad(format!("stage id {} is used twice (positions {prev} and {rank})", def.id));
                }
                let (intro, mut sections) = split_sections(&md);
                let hints = match sections.iter().position(|x| x.id == "hints") {
                    Some(i) => split_hints(&sections.remove(i).md),
                    None => Vec::new(),
                };
                let test_sources = crate::test_source::stage_test_sources(&root.join("template"), &def.tests);
                stages.push(Stage {
                    concepts: def.concepts,
                    concepts_optional: def.concepts_optional,
                    learn: def.learn,
                    beyond: def.beyond || def.kind == "challenge",
                    id: def.id,
                    title: def.title,
                    kind: def.kind,
                    difficulty: def.difficulty,
                    tests: def.tests,
                    renamed_from: def.renamed_from,
                    module: mt.code.clone(),
                    rank,
                    intro,
                    sections,
                    hints,
                    test_sources,
                });
            }
            modules.push(Module {
                planned: meta.planned_modules.contains(&mt.code),
                optional: mt.optional,
                beyond: mt.beyond.unwrap_or(mt.bustub.is_empty()),
                code: mt.code,
                title: mt.title,
                summary: mt.summary,
                project,
                lectures: mod_lectures,
                bustub: mt.bustub.iter().map(|f| format!("https://github.com/cmu-db/bustub/blob/master/{f}")).collect(),
                resources: mt.resources,
                stages,
            });
        }
        // checked over every module, published or not: an unpublished stage may already claim an id
        check_renamed_from(modules.iter().flat_map(|m| &m.stages).map(|s| (s.id.as_str(), s.renamed_from.as_slice()))).map_err(CourseError::Invalid)?;
        if let Some(only) = meta.published_modules.as_ref().filter(|_| only_published) {
            modules.retain(|m| only.contains(&m.code));
            let mut rank = 0;
            for st in modules.iter_mut().flat_map(|m| m.stages.iter_mut()) {
                rank += 1;
                st.rank = rank;
            }
        }
        let projects = meta.project.into_iter().map(|p| Project { number: p.number, title: p.title, planned: p.planned }).collect();
        let concepts = load_concepts(&root.join("concepts"))?;
        for st in modules.iter().flat_map(|m| m.stages.iter()) {
            if let Some(missing) = st.concepts.iter().chain(&st.concepts_optional).find(|c| !concepts.iter().any(|x| &x.id == *c)) {
                return bad(format!("stage {}: no concept {missing:?} in concepts/", st.id));
            }
        }
        Ok(Course { id: meta.id, title: meta.title, projects, modules, concepts })
    }

    pub fn stages(&self) -> impl Iterator<Item = &Stage> {
        self.modules.iter().flat_map(|m| m.stages.iter())
    }

    pub fn stage(&self, id: &str) -> Option<&Stage> {
        self.stages().find(|s| s.id == id)
    }

    /// The stage with this id or, for an id that is gone, the first stage that lists it in `renamed_from`.
    pub fn stage_or_successor(&self, id: &str) -> Option<&Stage> {
        self.stage(id).or_else(|| self.stages().find(|s| former_ids(&s.renamed_from).any(|old| old == id)))
    }

    /// The stages that count towards the course, in order: not challenges, in modules that are neither optional nor planned. The next
    /// stage to do is always one of these.
    pub fn core_stages(&self) -> impl Iterator<Item = &Stage> {
        self.modules.iter().filter(|m| !m.optional && !m.planned).flat_map(|m| m.stages.iter()).filter(|s| s.kind != "challenge")
    }

    /// The stage to go to after `id` passes: the next core stage (an optional module's stage leads on through its own module first). A
    /// challenge is a side trip: after one, the first core stage not yet `passed`.
    pub fn up_next(&self, id: &str, passed: impl Fn(&str) -> bool) -> Option<&Stage> {
        let here = self.stage(id)?;
        if here.kind == "challenge" {
            return self.core_stages().find(|s| !passed(&s.id));
        }
        let core: std::collections::HashSet<&str> = self.core_stages().map(|s| s.id.as_str()).collect();
        self.stages().skip_while(|s| s.id != id).skip(1).find(|s| core.contains(s.id.as_str()) || (s.module == here.module && s.kind != "challenge"))
    }

    pub fn concept(&self, id: &str) -> Option<&Concept> {
        self.concepts.iter().find(|c| c.id == id)
    }

    pub fn module(&self, code: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.code == code)
    }
}

/// Splits stage markdown at its `## ` headings (ignoring any inside code fences).
fn split_sections(md: &str) -> (String, Vec<Section>) {
    let mut intro = String::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut fence = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        if !fence && let Some(title) = line.strip_prefix("## ") {
            let title = title.trim().to_owned();
            let id: String = title.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
            sections.push(Section { id: id.trim_matches('-').to_owned(), title, md: String::new() });
            continue;
        }
        let target = sections.last_mut().map(|s| &mut s.md).unwrap_or(&mut intro);
        target.push_str(line);
        target.push('\n');
    }
    for s in &mut sections {
        s.md = s.md.trim().to_owned();
    }
    (intro.trim().to_owned(), sections)
}

fn load_concepts(dir: &Path) -> Result<Vec<Concept>> {
    let Ok(entries) = fs::read_dir(dir) else { return Ok(Vec::new()) };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "md")).collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let id = f.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
        let text = fs::read_to_string(&f).map_err(|e| CourseError::Invalid(format!("{}: {e}", f.display())))?;
        let (head, body) = match text.strip_prefix("---\n").and_then(|r| r.split_once("\n---\n")) {
            Some((h, b)) => (h, b),
            None => return bad(format!("{}: a concept starts with a --- header giving title, summary and minutes", f.display())),
        };
        let field = |k: &str| head.lines().find_map(|l| l.strip_prefix(&format!("{k}:")).map(|v| v.trim().trim_matches('"').to_owned()));
        let title = field("title").ok_or_else(|| CourseError::Invalid(format!("{}: no title", f.display())))?;
        let summary = field("summary").unwrap_or_default();
        let minutes = field("minutes").and_then(|m| m.parse().ok()).unwrap_or(5);
        let (intro, mut sections) = split_sections(body);
        if !intro.is_empty() {
            sections.insert(0, Section { id: "intro".into(), title: "Overview".into(), md: intro });
        }
        out.push(Concept { id, title, summary, minutes, sections });
    }
    Ok(out)
}

/// The `### ` blocks of a hints section.
fn split_hints(md: &str) -> Vec<Hint> {
    let mut hints: Vec<Hint> = Vec::new();
    let mut fence = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        if !fence && let Some(title) = line.strip_prefix("### ") {
            hints.push(Hint { title: title.trim().to_owned(), md: String::new() });
            continue;
        }
        if let Some(h) = hints.last_mut() {
            h.md.push_str(line);
            h.md.push('\n');
        }
    }
    for h in &mut hints {
        h.md = h.md.trim().to_owned();
    }
    hints
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(|e| CourseError::Invalid(format!("reading {}: {e}", path.display())))?;
    toml::from_str(&text).map_err(|e| CourseError::Invalid(format!("parsing {}: {e}", path.display())))
}

fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| CourseError::Invalid(format!("reading {}: {e}", dir.display())))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_at_headings_but_not_inside_code() {
        let md = "Intro line.\n\n## The task\n\nDo it.\n\n```sh\n## not a heading\n```\n\n## Notes\n\nA note.\n";
        let (intro, sections) = split_sections(md);
        assert_eq!(intro, "Intro line.");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].id, "the-task");
        assert!(sections[0].md.contains("## not a heading"));
        assert_eq!(sections[1].md, "A note.");
    }

    #[test]
    fn hints_come_out_of_the_hints_section() {
        let md = "Intro.\n\n## The task\n\nDo it.\n\n## Hints\n\n### Mind the order\n\nLatch the parent first.\n\n### The trap\n\nDon't hold two.\n";
        let (_, mut sections) = split_sections(md);
        let i = sections.iter().position(|s| s.id == "hints").unwrap();
        let hints = split_hints(&sections.remove(i).md);
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].title, "Mind the order");
        assert_eq!(hints[1].md, "Don't hold two.");
        assert_eq!(sections.len(), 1);
    }

    #[test]
    fn the_shipped_course_loads() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../courses/bustub");
        let c = Course::load(&root).expect("courses/bustub loads");
        assert!(c.stages().count() >= 4, "{} stages", c.stages().count());
        assert!(c.stage("1a-01").is_some_and(|s| !s.sections.is_empty()));
    }

    /// A one-stage course in a temporary directory, with the given `stage.toml` lines.
    fn tiny_course(tag: &str, stage_extra: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("anneal-course-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let stage = root.join("modules/01-m/stages/01-s");
        fs::create_dir_all(&stage).unwrap();
        fs::create_dir_all(root.join("concepts")).unwrap();
        fs::write(root.join("course.toml"), "id = \"t\"\ntitle = \"T\"\n\n[[project]]\nnumber = 1\ntitle = \"One\"\n").unwrap();
        fs::write(root.join("modules/01-m/module.toml"), "code = \"1a\"\ntitle = \"M\"\n").unwrap();
        fs::write(stage.join("stage.toml"), format!("id = \"1a-01\"\ntitle = \"S\"\nkind = \"build\"\ndifficulty = \"easy\"\ntests = [\"t\"]\n{stage_extra}")).unwrap();
        fs::write(stage.join("stage.md"), "Intro.\n\n## The task\n\nDo it.\n").unwrap();
        for id in ["needed", "extra"] {
            fs::write(root.join(format!("concepts/{id}.md")), format!("---\ntitle: {id}\nsummary: s\nminutes: 3\n---\nBody.\n")).unwrap();
        }
        root
    }

    /// A course with an optional module r (two stages and a challenge), module 1a (two stages and a challenge) and module 1b.
    fn three_modules(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("anneal-course-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("concepts")).unwrap();
        fs::write(root.join("course.toml"), "id = \"t\"\ntitle = \"T\"\n").unwrap();
        for (dir, code, extra, stages) in [
            ("00-r", "r", "project = 0\noptional = true\n", &[("01", "r-01", "learn"), ("02", "r-02", "learn"), ("90", "r-c1", "challenge")][..]),
            ("01-a", "1a", "", &[("01", "1a-01", "build"), ("02", "1a-02", "boss"), ("90", "1a-c1", "challenge")][..]),
            ("02-b", "1b", "", &[("01", "1b-01", "build")][..]),
        ] {
            fs::create_dir_all(root.join("modules").join(dir)).unwrap();
            fs::write(root.join("modules").join(dir).join("module.toml"), format!("code = \"{code}\"\ntitle = \"M\"\n{extra}")).unwrap();
            for (sd, id, kind) in stages {
                let st = root.join("modules").join(dir).join("stages").join(sd);
                fs::create_dir_all(&st).unwrap();
                fs::write(st.join("stage.toml"), format!("id = \"{id}\"\ntitle = \"S\"\nkind = \"{kind}\"\ndifficulty = \"easy\"\ntests = [\"t\"]\n")).unwrap();
                fs::write(st.join("stage.md"), "Intro.\n").unwrap();
            }
        }
        root
    }

    #[test]
    fn core_stages_and_what_comes_next_skip_challenges_and_optional_modules() {
        let root = three_modules("next");
        let c = Course::load(&root).expect("loads");
        let ids = |v: Vec<&Stage>| v.into_iter().map(|s| s.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(c.core_stages().collect()), ["1a-01", "1a-02", "1b-01"]);
        let next = |id: &str, passed: &[&str]| c.up_next(id, |x| passed.contains(&x)).map(|s| s.id.clone());
        assert_eq!(next("1a-02", &[]).as_deref(), Some("1b-01"), "after a boss: the next module, not its challenge");
        assert_eq!(next("r-01", &[]).as_deref(), Some("r-02"), "an optional module leads on through itself");
        assert_eq!(next("r-02", &[]).as_deref(), Some("1a-01"), "then onto the path");
        assert_eq!(next("1a-c1", &["1a-01"]).as_deref(), Some("1a-02"), "after a challenge: where the learner is");
        assert_eq!(next("1b-01", &[]), None, "the last stage has none");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_stage_lists_required_and_optional_concepts() {
        let root = tiny_course("opt", "concepts = [\"needed\"]\nconcepts_optional = [\"extra\"]\n");
        let c = Course::load(&root).expect("loads");
        let s = c.stage("1a-01").unwrap();
        assert_eq!(s.concepts, ["needed"]);
        assert_eq!(s.concepts_optional, ["extra"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_optional_concept_that_does_not_exist_is_an_error() {
        let root = tiny_course("optbad", "concepts_optional = [\"nope\"]\n");
        let err = Course::load(&root).expect_err("a missing optional concept is refused").to_string();
        assert!(err.contains("nope"), "{err}");
        let _ = fs::remove_dir_all(root);
    }

    /// A one-module course of the given stages, each `(id, extra stage.toml lines)`.
    fn course_with(tag: &str, stages: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("anneal-course-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("modules/01-a")).unwrap();
        fs::write(root.join("course.toml"), "id = \"t\"\ntitle = \"T\"\n").unwrap();
        fs::write(root.join("modules/01-a/module.toml"), "code = \"1a\"\ntitle = \"M\"\n").unwrap();
        for (i, (id, extra)) in stages.iter().enumerate() {
            let st = root.join(format!("modules/01-a/stages/{:02}", i + 1));
            fs::create_dir_all(&st).unwrap();
            fs::write(st.join("stage.toml"), format!("id = \"{id}\"\ntitle = \"S\"\nkind = \"build\"\ndifficulty = \"easy\"\ntests = [\"t\"]\n{extra}")).unwrap();
            fs::write(st.join("stage.md"), "Intro.\n").unwrap();
        }
        root
    }

    #[test]
    fn renamed_from_takes_ids_and_chains_of_ids() {
        let root = course_with(
            "renamed",
            &[("1a-01", "renamed_from = [\"0x-01\"]\n"), ("1a-02", "renamed_from = [\"0x-02\", [\"0y-03\", \"0x-03\"]]\n"), ("1a-03", "")],
        );
        let c = Course::load(&root).expect("loads");
        assert_eq!(c.stage("1a-01").unwrap().renamed_from, [FormerStage::Id("0x-01".into())]);
        let merged = &c.stage("1a-02").unwrap().renamed_from;
        assert_eq!(former_ids(merged).collect::<Vec<_>>(), ["0x-02", "0y-03", "0x-03"]);
        assert!(c.stage("1a-03").unwrap().renamed_from.is_empty(), "optional: none by default");
        assert_eq!(c.stage_or_successor("0y-03").map(|s| s.id.as_str()), Some("1a-02"), "an old id leads to the stage that took it over");
        assert_eq!(c.stage_or_successor("1a-03").map(|s| s.id.as_str()), Some("1a-03"));
        assert!(c.stage_or_successor("0z-01").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_former_id_may_not_be_a_current_stage_or_listed_twice() {
        let root = course_with("renamed-current", &[("1a-01", ""), ("1a-02", "renamed_from = [\"1a-01\"]\n")]);
        let err = Course::load(&root).expect_err("a current id is refused").to_string();
        assert!(err.contains("1a-02") && err.contains("1a-01") && err.contains("current stage"), "{err}");
        let root2 = course_with("renamed-twice", &[("1a-01", "renamed_from = [\"0x-01\", \"0x-01\"]\n")]);
        let err = Course::load(&root2).expect_err("a repeated entry is refused").to_string();
        assert!(err.contains("0x-01") && err.contains("twice"), "{err}");
        let root4 = course_with("renamed-twice-in-chain", &[("1a-01", "renamed_from = [[\"0x-02\", \"0x-01\", \"0x-02\"]]\n")]);
        assert!(Course::load(&root4).is_err(), "an id twice in one chain is refused");
        let root5 = course_with("renamed-after-merge", &[("1a-01", "renamed_from = [[\"0y-01\", \"0x-01\"], [\"0y-01\", \"0x-02\"]]\n")]);
        Course::load(&root5).expect("a merged stage renamed again keeps one entry per half");
        let root3 = course_with("renamed-empty", &[("1a-01", "renamed_from = [[]]\n")]);
        assert!(Course::load(&root3).is_err(), "an empty chain is refused");
        for r in [root, root2, root3, root4, root5] {
            let _ = fs::remove_dir_all(r);
        }
    }

    #[test]
    fn the_parts_of_a_split_may_each_list_the_stage_they_came_from() {
        let root = course_with("split", &[("1a-01", "renamed_from = [\"0x-01\"]\n"), ("1a-02", "renamed_from = [\"0x-01\"]\n"), ("1a-03", "renamed_from = [\"0x-01\"]\n")]);
        let c = Course::load(&root).expect("a split loads");
        assert!(c.stages().all(|s| former_ids(&s.renamed_from).eq(["0x-01"])));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_unpublished_stage_claiming_a_published_id_is_still_refused() {
        let root = course_with("renamed-unpublished", &[("1a-01", "")]);
        fs::write(root.join("course.toml"), "id = \"t\"\ntitle = \"T\"\npublished_modules = [\"1a\"]\n").unwrap();
        let hidden = root.join("modules/02-b/stages/01");
        fs::create_dir_all(&hidden).unwrap();
        fs::write(root.join("modules/02-b/module.toml"), "code = \"1b\"\ntitle = \"M\"\n").unwrap();
        fs::write(hidden.join("stage.toml"), "id = \"1b-01\"\ntitle = \"S\"\nkind = \"build\"\ndifficulty = \"easy\"\ntests = [\"t\"]\nrenamed_from = [\"1a-01\"]\n").unwrap();
        fs::write(hidden.join("stage.md"), "Intro.\n").unwrap();
        assert!(Course::load(&root).is_err(), "checked across every module, not only the published ones");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_stage_passes_through_its_former_stages_only_when_every_one_passed() {
        let one = |id: &str| FormerStage::Id(id.into());
        let passed = |ids: &'static [&'static str]| move |id: &str| ids.contains(&id);
        assert!(passed_through_former(&[one("a")], passed(&["a"])), "a rename");
        assert!(!passed_through_former(&[], passed(&["a"])), "a stage that replaces nothing passes only on its own");
        assert!(passed_through_former(&[one("a"), one("b")], passed(&["a", "b"])), "a merge of two passed stages");
        assert!(!passed_through_former(&[one("a"), one("b")], passed(&["a"])), "a merge needs both halves");
        let chain = FormerStage::Ids(vec!["b2".into(), "b1".into()]);
        assert!(passed_through_former(&[one("a"), chain.clone()], passed(&["a", "b1"])), "a stage renamed twice: passed under its first id");
        assert!(!passed_through_former(&[one("a"), chain], passed(&["b2"])));
        let merged_then_renamed = [FormerStage::Ids(vec!["m".into(), "x".into()]), FormerStage::Ids(vec!["m".into(), "y".into()])];
        assert!(passed_through_former(&merged_then_renamed, passed(&["m"])), "the merged stage passed under its own id");
        assert!(passed_through_former(&merged_then_renamed, passed(&["x", "y"])), "or both its halves did");
        assert!(!passed_through_former(&merged_then_renamed, passed(&["x"])));
    }

    #[test]
    fn each_former_stage_takes_the_state_of_its_newest_id_that_has_one() {
        let former = [FormerStage::Ids(vec!["new".into(), "old".into()]), FormerStage::Id("other".into())];
        let stored = |id: &str| match id {
            "new" => Some(2),
            "old" => Some(1),
            _ => None,
        };
        assert_eq!(former_states(&former, stored), [Some(2), None]);
        assert_eq!(former_states(&former, |id| (id == "old").then_some(1)), [Some(1), None]);
    }
}
