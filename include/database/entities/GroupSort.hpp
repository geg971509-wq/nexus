#pragma once

// Sort method shared by the Group entity (SortProfiles) and the mainwindow UI.
namespace GroupSortMethod {
    enum GroupSortMethod {
        Raw,
        ByType,
        ByAddress,
        ByName,
        ByTestResult,
        ById,
        ByTraffic,
        BySecurity,
    };
}

struct GroupSortAction {
    GroupSortMethod::GroupSortMethod method = GroupSortMethod::Raw;
    bool descending = false; // 默认升序，开这个就是降序
};
