import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { MotionGlobalConfig } from "motion/react";
import { afterEach } from "vitest";

// jsdom has no layout or animation engine: finish every animation instantly so tests assert on
// the final screen instead of racing exit transitions.
MotionGlobalConfig.skipAnimations = true;

afterEach(cleanup);
