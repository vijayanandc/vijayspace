package com.vijay;

import java.util.ArrayList;

public class TestUtil {

	public static String getWelComeString(){
		return "This is a welcome string";
	}
	
	public static ArrayList<String> getTabNames(){
		ArrayList<String> list = new ArrayList<String>();
		
		list.add("Home");
		list.add("Issues");
		list.add("Support");
		list.add("Payment");
		list.add("Mobiles");
		list.add("Videos");
		
		return list;
	}
}
