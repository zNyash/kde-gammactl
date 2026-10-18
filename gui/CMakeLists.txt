cmake_minimum_required(VERSION 3.21)
project(kde-gammactl-gui LANGUAGES CXX)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2)
qt_policy(SET QTP0001 NEW)
qt_standard_project_setup(REQUIRES 6.5)

qt_add_executable(kde-gammactl-gui main.cpp)

qt_add_qml_module(kde-gammactl-gui
    URI GammaPanel
    VERSION 1.0
    QML_FILES Main.qml
)

target_link_libraries(kde-gammactl-gui
    PRIVATE
        Qt6::Core
        Qt6::Gui
        Qt6::Qml
        Qt6::Quick
        Qt6::QuickControls2
)

install(TARGETS kde-gammactl-gui RUNTIME DESTINATION bin)
