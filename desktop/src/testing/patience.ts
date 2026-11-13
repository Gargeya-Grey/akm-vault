import { configure } from "@testing-library/react";

// The one bound of every wait for something that must happen in a page test (class Q). The
// library's own bound is one second, and a loaded run needs more for the first render of a file.
// A wait that passes returns at once, so the bound costs time only when a test fails.
configure({ asyncUtilTimeout: 10_000 });
