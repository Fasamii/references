print("\t\t\x1b[38;5;4m[FROM LUA]> LUA EXECUTING RN\x1b[0m");

Config = {
	on_new_output = function(state)
		local state_str = "";
		if state then
			state_str = "Connected";
		end
		if not state then
			state_str = "Disconnected";
		end
		print("\t\tskibidi from Config in lua - with state: " .. state_str);
	end
}
