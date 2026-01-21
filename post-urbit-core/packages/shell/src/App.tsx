import React from "react";
import MenuBar from "./components/system7/MenuBar";
import Window from "./components/system7/Window";
import Button from "./components/system7/Button";
import Checkbox from "./components/system7/Checkbox";
import Radio from "./components/system7/Radio";
import TextInput from "./components/system7/TextInput";
import Dropdown from "./components/system7/Dropdown";
import Slider from "./components/system7/Slider";
import ProgressBar from "./components/system7/ProgressBar";
import Alert from "./components/system7/Alert";
import AppGrid from "./components/shell/AppGrid";
import StatusBar from "./components/shell/StatusBar";
import PermissionPrompt from "./components/shell/PermissionPrompt";

const App = () => {
  return (
    <div className="s7-shell">
      <MenuBar />
      <div className="s7-desktop">
        <div className="s7-desktop-grid">
          <AppGrid />
        </div>
        <div className="s7-desktop-right">
          <Window title="System 7 Controls">
            <div className="s7-form-grid">
              <div className="s7-form-row">
                <Button label="Standard" />
                <Button label="Default" variant="default" />
                <Button label="Pressed" pressed />
              </div>
              <div className="s7-form-row">
                <Checkbox label="Enable sound" checked />
                <Checkbox label="Mixed state" mixed />
                <Checkbox label="Disabled" />
              </div>
              <div className="s7-form-row">
                <Radio label="Option A" selected />
                <Radio label="Option B" />
              </div>
              <div className="s7-form-row">
                <TextInput placeholder="Type here" value="System 7 input" />
              </div>
              <div className="s7-form-row">
                <Dropdown label="Preferred Network" options={["LocalTalk", "Ethernet", "Offline"]} />
              </div>
              <div className="s7-form-row">
                <Slider label="Volume" />
              </div>
              <div className="s7-form-row">
                <ProgressBar value={65} />
                <ProgressBar indeterminate />
              </div>
            </div>
          </Window>
          <div className="s7-dialogs">
            <Alert
              type="stop"
              title="System error"
              message="This action could not be completed."
            />
            <Alert
              type="caution"
              title="Low disk space"
              message="Archive files or empty the Trash to free memory."
            />
            <Alert
              type="note"
              title="Update complete"
              message="The system has finished installing updates."
            />
            <PermissionPrompt />
          </div>
        </div>
      </div>
      <StatusBar />
    </div>
  );
};

export default App;
