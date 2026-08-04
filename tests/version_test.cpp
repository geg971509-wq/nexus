#include "include/global/Version.hpp"

#include <QDebug>

#include <array>
#include <cstdlib>

int main() {
    struct TestCase {
        const char *candidate;
        const char *current;
        bool expected;
    };

    constexpr std::array cases{
        TestCase{"4.3.7", "4.3.6", true},
        TestCase{"4.10.0", "4.9.9", true},
        TestCase{"4.2.0", "4.2.0-beta.1", true},
        TestCase{"4.2.0-beta.1", "4.2.0-alpha.9", true},
        TestCase{"4.2.0-rc.1", "4.2.0-beta.9", true},
        TestCase{"4.2.0", "4.2.0-rc.9", true},
        TestCase{"4.2.0-beta.2", "4.2.0-beta.1", true},
        TestCase{"4.2.0-beta.1", "4.2.0", false},
        TestCase{"4.1.9-rc.99", "4.2.0-alpha.1", false},
        TestCase{"4.2.0", "4.2.0", false},
        TestCase{"invalid", "4.2.0", false},
        TestCase{"4.2", "4.1.0", false},
        TestCase{"4.2.0-beta", "4.1.0", false},
        TestCase{"4.2.0-beta.-1", "4.1.0", false},
        TestCase{"4.2.0-beta.x", "4.1.0", false},
        TestCase{"4.2.0.1", "4.1.0", false},
        TestCase{"v4.2.0", "4.1.0", false},
        TestCase{"4.2.0+build.1", "4.1.0", false},
        TestCase{"Throne-4.3.7-macos-arm64.zip", "4.3.6", false},
        TestCase{"4.3.7", "", false},
    };

    for (const auto &test : cases) {
        const auto actual = Throne::IsVersionNewer(test.candidate, test.current);
        if (actual != test.expected) {
            qCritical() << test.candidate << "vs" << test.current << "expected" << test.expected << "got" << actual;
            return EXIT_FAILURE;
        }
    }
    return EXIT_SUCCESS;
}
