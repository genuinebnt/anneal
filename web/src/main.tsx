import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Navigate, Outlet, RouterProvider, createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { DsaPage } from "./pages/DsaPage";
import { PatternIdea } from "./pages/PatternIdea";
import { PatternProblems } from "./pages/PatternProblems";
import { CalendarPage } from "./pages/CalendarPage";
import { DsaPlanPage } from "./pages/DsaPlanPage";
import { MockPage } from "./pages/MockPage";
import { DsaProblemPage } from "./pages/DsaProblemPage";
import { PatternPage } from "./pages/PatternPage";
import { ReviewPage } from "./pages/ReviewPage";
import { PracticePage } from "./pages/PracticePage";
import { LoginPage } from "./pages/LoginPage";
import { ProgressPage } from "./pages/ProgressPage";
import { SectionPage } from "./pages/SectionPage";
import { TrackPage } from "./pages/TrackPage";
import { COURSE_ID, CoursePage } from "./pages/CoursePage";
import { CourseStagePage } from "./pages/CourseStagePage";
import { CourseConceptPage } from "./pages/CourseConceptPage";
import { Workspace } from "./workspace/Workspace";
import { installPressEffects } from "./components/kit";
import { KitPage } from "./pages/KitPage";
import { ToastHost } from "./components/toasts";
import { hydratePrefs } from "./prefs";
import { applyUi } from "./uiprefs";
import { ShortcutHelp } from "./components/ShortcutHelp";
import { Tips } from "./components/Tips";
import { WelcomeBack } from "./components/WelcomeBack";
import { ConnectionBanner } from "./components/ConnectionBanner";
import { ErrorScreen } from "./components/ErrorScreen";
import { SettingsPage } from "./pages/SettingsPage";
import { LogPage } from "./pages/LogPage";
import { CompanyPage } from "./pages/CompanyPage";
import "./styles/design.css";
import "./styles/app.css";
import "./styles/catalog.css";
import "./styles/progress.css";
import "./styles/dsa.css";
import "./styles/mock.css";
import "./styles/lessons.css";
import "./styles/review.css";
import "./styles/problem.css";
import "./styles/calendar.css";
import "./styles/kit.css";
import "./styles/course.css";
import "./styles/mock-course.css";
import "./styles/mock-section.css";
import "./styles/mock-extra.css";
import "./styles/polish.css";
import "./styles/focus-timer.css";
import "./styles/settings.css";
import "./styles/ui.css";
import "./styles/idea.css";
import "./styles/learn.css";
import "./styles/loops.css";
import "./styles/coursemap.css";
import "./styles/streaks.css";
import "./styles/patterncards.css";

const rootRoute = createRootRoute({
  component: () => (
    <>
      <Outlet />
      <ToastHost />
      <ShortcutHelp />
      <WelcomeBack />
      <Tips />
      <ConnectionBanner />
    </>
  ),
});

const routes = [
  // Today is designed but needs the review queue (a later phase); start on DSA until then.
  createRoute({ getParentRoute: () => rootRoute, path: "/", component: () => <Navigate to="/dsa" /> }),
  createRoute({ getParentRoute: () => rootRoute, path: "/login", component: LoginPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/dsa", component: DsaPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/dsa/plan", component: DsaPlanPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/dsa/calendar", component: CalendarPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/dsa/mock", component: MockPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/dsa/review", component: ReviewPage }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/dsa/patterns/$code",
    component: function Patterns() {
      const { code } = patternsRoute.useParams();
      return <PatternPage code={code} />;
    },
  }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/dsa/practice/$code",
    component: function Practice() {
      const { code } = practiceRoute.useParams();
      return <PracticePage code={code} />;
    },
  }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/d/$slug",
    component: function DsaProblem() {
      const { slug } = dsaProblemRoute.useParams();
      return <DsaProblemPage key={slug} slug={slug} />;
    },
  }),
  createRoute({ getParentRoute: () => rootRoute, path: "/rust", component: () => <SectionPage area="rust" /> }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/t/$track",
    component: function Track() {
      const { track } = trackRoute.useParams();
      return <TrackPage slug={track} />;
    },
  }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/p/$id",
    component: function Problem() {
      const { id } = problemRoute.useParams();
      return <Workspace id={id} />;
    },
  }),
  createRoute({ getParentRoute: () => rootRoute, path: "/progress", component: ProgressPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/courses", component: () => <CoursePage course={COURSE_ID} /> }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/courses/$course",
    component: function CourseRoute() {
      const { course } = courseRoute.useParams();
      return <CoursePage course={course} />;
    },
  }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/courses/$course/$stage",
    component: function CourseStageRoute() {
      const { course, stage } = courseStageRoute.useParams();
      return <CourseStagePage key={stage} course={course} stage={stage} />;
    },
  }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/courses/$course/concept/$id",
    component: function CourseConceptRoute() {
      const { course, id } = courseConceptRoute.useParams();
      return <CourseConceptPage key={id} course={course} id={id} />;
    },
  }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/dsa/patterns/$code/problems",
    component: function PatternProblemsRoute() {
      const { code } = patternProblemsRoute.useParams();
      return <PatternProblems code={code} />;
    },
  }),
  createRoute({ getParentRoute: () => rootRoute, path: "/kit", component: KitPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/settings", component: SettingsPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/log", component: LogPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/dsa/company", component: CompanyPage }),
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/dsa/patterns/$code/idea",
    component: function PatternIdeaRoute() {
      const { code } = patternIdeaRoute.useParams();
      return <PatternIdea code={code} />;
    },
  }),
] as const;
const patternsRoute = routes[7];
const practiceRoute = routes[8];
const dsaProblemRoute = routes[9];
const trackRoute = routes[11];
const problemRoute = routes[12];
const courseRoute = routes[15];
const courseStageRoute = routes[16];
const courseConceptRoute = routes[17];
const patternProblemsRoute = routes[18];
const patternIdeaRoute = routes[23];

// Pages are preloaded when their link is hovered, scroll positions come back with Back, and a page that throws shows a way out instead of a blank screen.
const router = createRouter({ routeTree: rootRoute.addChildren([...routes]), defaultPreload: "intent", defaultPreloadStaleTime: 30_000, scrollRestoration: true, defaultErrorComponent: ErrorScreen });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 5_000, refetchOnWindowFocus: false } },
});

installPressEffects();
void hydratePrefs().then(() => {
  applyUi();
  return createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </StrictMode>,
  );
});
