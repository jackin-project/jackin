import SwiftUI

struct ContentView: View {
    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "terminal")
                .font(.system(size: 48))
                .foregroundStyle(.green)
            Text("jackin'")
                .font(.largeTitle.bold())
            Text("Desktop — milestone 1: the window runs.")
                .foregroundStyle(.secondary)
        }
        .frame(minWidth: 560, minHeight: 360)
        .padding(40)
    }
}

#Preview {
    ContentView()
}
